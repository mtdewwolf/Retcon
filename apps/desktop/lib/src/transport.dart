import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'generated/protocol_v1.dart';

/// Align with retcon-protocol::MAX_FRAME_BYTES.
const int maxFrameBytes = 1024 * 1024;

/// A newline-delimited JSON protocol connection.
abstract interface class ProtocolConnection {
  Stream<String> get lines;
  Future<void> writeLine(String line);
  Future<void> close();
}

/// Open a transport connection using discovery metadata.
Future<ProtocolConnection> openTransport(Discovery discovery) async {
  if (discovery.transport == 'unix_socket') {
    return UnixProtocolConnection.connect(discovery.path);
  }
  if (Platform.isWindows) {
    return PipeProtocolConnection.connect(discovery.path);
  }
  throw UnsupportedError(
    'Named pipe transport is only supported on Windows desktop builds.',
  );
}

StreamTransformer<String, String> _cappedLines() {
  return StreamTransformer.fromHandlers(
    handleData: (line, sink) {
      if (utf8.encode(line).length > maxFrameBytes) {
        sink.addError(
          StateError('inbound frame exceeds $maxFrameBytes bytes'),
        );
        return;
      }
      sink.add(line);
    },
  );
}

final class UnixProtocolConnection implements ProtocolConnection {
  UnixProtocolConnection._(this._socket, this._lines);

  final Socket _socket;
  final Stream<String> _lines;

  static Future<UnixProtocolConnection> connect(String path) async {
    final socket = await Socket.connect(
      InternetAddress(path, type: InternetAddressType.unix),
      0,
      timeout: const Duration(seconds: 3),
    );
    final lines = socket
        .cast<List<int>>()
        .transform(utf8.decoder)
        .transform(const LineSplitter())
        .transform(_cappedLines())
        .asBroadcastStream();
    return UnixProtocolConnection._(socket, lines);
  }

  @override
  Stream<String> get lines => _lines;

  @override
  Future<void> writeLine(String line) async {
    _socket.writeln(line);
    await _socket.flush();
  }

  @override
  Future<void> close() async {
    _socket.destroy();
  }
}

final class PipeProtocolConnection implements ProtocolConnection {
  PipeProtocolConnection._(this._file, this._controller, this._timer);

  final RandomAccessFile _file;
  final StreamController<String> _controller;
  final Timer _timer;
  final StringBuffer _buffer = StringBuffer();

  static Future<PipeProtocolConnection> connect(String path) async {
    final file = await File(path).open(mode: FileMode.write);
    final controller = StreamController<String>.broadcast();
    final connection = PipeProtocolConnection._(
      file,
      controller,
      Timer.periodic(const Duration(milliseconds: 20), (_) {}),
    );
    connection._timer.cancel();
    final timer = Timer.periodic(const Duration(milliseconds: 20), (_) {
      unawaited(connection._poll());
    });
    return PipeProtocolConnection._(file, controller, timer);
  }

  Future<void> _poll() async {
    try {
      final chunk = await _file.read(4096);
      if (chunk.isEmpty) return;
      if (_buffer.length + chunk.length > maxFrameBytes + 1) {
        _buffer.clear();
        if (!_controller.isClosed) {
          _controller.addError(
            StateError('inbound frame exceeds $maxFrameBytes bytes'),
          );
        }
        return;
      }
      _buffer.write(utf8.decode(chunk, allowMalformed: true));
      final text = _buffer.toString();
      final parts = text.split('\n');
      _buffer.clear();
      if (!text.endsWith('\n') && parts.isNotEmpty) {
        final remainder = parts.removeLast();
        if (utf8.encode(remainder).length > maxFrameBytes) {
          if (!_controller.isClosed) {
            _controller.addError(
              StateError('inbound frame exceeds $maxFrameBytes bytes'),
            );
          }
          return;
        }
        _buffer.write(remainder);
      }
      for (final part in parts) {
        if (part.isEmpty) continue;
        if (utf8.encode(part).length > maxFrameBytes) {
          if (!_controller.isClosed) {
            _controller.addError(
              StateError('inbound frame exceeds $maxFrameBytes bytes'),
            );
          }
          continue;
        }
        _controller.add(part);
      }
    } on FileSystemException {
      await _controller.close();
    }
  }

  @override
  Stream<String> get lines => _controller.stream;

  @override
  Future<void> writeLine(String line) async {
    _file.writeStringSync('$line\n');
    await _file.flush();
  }

  @override
  Future<void> close() async {
    _timer.cancel();
    await _controller.close();
    await _file.close();
  }
}
