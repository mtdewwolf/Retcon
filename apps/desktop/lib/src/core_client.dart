import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:logging/logging.dart';

final _log = Logger('retcon.desktop.core');

enum CoreConnectionStatus { disconnected, connecting, connected, reconnecting }

class CoreRpcException implements Exception {
  CoreRpcException(this.message, [this.details]);
  final String message;
  final Object? details;
  @override
  String toString() => message;
}

/// Temporary typed-enough client for the Phase 2 newline-delimited JSON spike.
/// The generated Phase 4 protocol replaces this file.
class CoreClient extends ChangeNotifier {
  CoreClient({Directory? dataDirectory})
    : dataDirectory = dataDirectory ?? _defaultDataDirectory();

  final Directory dataDirectory;
  final _events = StreamController<Map<String, dynamic>>.broadcast();
  final _pending = <int, Completer<Map<String, dynamic>>>{};
  Socket? _socket;
  StreamSubscription<String>? _lines;
  Timer? _heartbeat;
  bool _closing = false;
  bool _reconnectScheduled = false;
  int _nextId = 0;
  int _generation = 0;
  CoreConnectionStatus _status = CoreConnectionStatus.disconnected;

  CoreConnectionStatus get status => _status;
  Stream<Map<String, dynamic>> get events => _events.stream;

  Future<void> connect({bool launchIfNeeded = true}) async {
    if (_status == CoreConnectionStatus.connected ||
        _status == CoreConnectionStatus.connecting) {
      return;
    }
    _setStatus(
      _generation == 0
          ? CoreConnectionStatus.connecting
          : CoreConnectionStatus.reconnecting,
    );
    _closing = false;
    try {
      var discovery = await _readDiscovery();
      if (discovery == null && launchIfNeeded) {
        await _launchCore();
        discovery = await _waitForDiscovery();
      }
      if (discovery == null) {
        throw CoreRpcException('Retcon Core is not running.');
      }
      final address = (discovery['address'] as String).split(':');
      final socket = await Socket.connect(
        address.first,
        int.parse(address.last),
        timeout: const Duration(seconds: 3),
      );
      _socket = socket;
      socket.writeln(jsonEncode({'auth': discovery['token']}));
      _lines = socket
          .cast<List<int>>()
          .transform(utf8.decoder)
          .transform(const LineSplitter())
          .listen(
            _handleLine,
            onError: _handleDisconnect,
            onDone: _handleDisconnect,
          );
      _generation++;
      _setStatus(CoreConnectionStatus.connected);
      _heartbeat?.cancel();
      _heartbeat = Timer.periodic(const Duration(seconds: 10), (_) {
        request(
          'core.health',
          timeout: const Duration(seconds: 3),
        ).catchError((Object error) => <String, dynamic>{});
      });
      await request('core.health');
    } catch (error) {
      _setStatus(CoreConnectionStatus.disconnected);
      rethrow;
    }
  }

  Future<Map<String, dynamic>> request(
    String method, {
    Map<String, dynamic> params = const {},
    Duration timeout = const Duration(seconds: 30),
  }) async {
    if (_socket == null) throw CoreRpcException('Retcon Core is disconnected.');
    final id = ++_nextId;
    final completer = Completer<Map<String, dynamic>>();
    _pending[id] = completer;
    _socket!.writeln(
      jsonEncode({'id': id, 'method': method, 'params': params}),
    );
    try {
      return await completer.future.timeout(timeout);
    } on TimeoutException {
      throw CoreRpcException('$method timed out after ${timeout.inSeconds}s.');
    } finally {
      _pending.remove(id);
    }
  }

  void _handleLine(String line) {
    final value = jsonDecode(line) as Map<String, dynamic>;
    final event = value['event'];
    if (event is Map<String, dynamic>) {
      _events.add(event);
      return;
    }
    final id = value['id'];
    final completer = id is int ? _pending.remove(id) : null;
    if (completer == null) return;
    if (value['error'] != null) {
      final error = value['error'] as Map<String, dynamic>;
      completer.completeError(
        CoreRpcException(
          error['user_message']?.toString() ?? 'Core request failed.',
          error,
        ),
      );
    } else {
      completer.complete(
        (value['result'] as Map?)?.cast<String, dynamic>() ?? {},
      );
    }
  }

  void _handleDisconnect([Object? error]) {
    if (_closing || _reconnectScheduled) return;
    _log.warning('core connection closed', error);
    _socket = null;
    for (final pending in _pending.values) {
      if (!pending.isCompleted) {
        pending.completeError(CoreRpcException('Core connection closed.'));
      }
    }
    _pending.clear();
    _setStatus(CoreConnectionStatus.reconnecting);
    _reconnectScheduled = true;
    Future<void>.delayed(const Duration(milliseconds: 500), () async {
      _reconnectScheduled = false;
      if (_closing) return;
      try {
        await connect();
      } catch (_) {
        if (!_closing) {
          Future<void>.delayed(const Duration(seconds: 2), _handleDisconnect);
        }
      }
    });
  }

  Future<Map<String, dynamic>?> _readDiscovery() async {
    final file = File(
      '${dataDirectory.path}${Platform.pathSeparator}core.json',
    );
    if (!await file.exists()) return null;
    try {
      return (jsonDecode(await file.readAsString()) as Map)
          .cast<String, dynamic>();
    } catch (_) {
      return null;
    }
  }

  Future<Map<String, dynamic>?> _waitForDiscovery() async {
    for (var attempt = 0; attempt < 50; attempt++) {
      final value = await _readDiscovery();
      if (value != null) return value;
      await Future<void>.delayed(const Duration(milliseconds: 100));
    }
    return null;
  }

  Future<void> _launchCore() async {
    await dataDirectory.create(recursive: true);
    final override = Platform.environment['RETCON_CORE_PATH'];
    final candidates = [
      ?override,
      '${Directory.current.path}${Platform.pathSeparator}target${Platform.pathSeparator}debug${Platform.pathSeparator}retcon-core.exe',
      '${Directory.current.path}${Platform.pathSeparator}..${Platform.pathSeparator}..${Platform.pathSeparator}target${Platform.pathSeparator}debug${Platform.pathSeparator}retcon-core.exe',
      '${File(Platform.resolvedExecutable).parent.path}${Platform.pathSeparator}retcon-core.exe',
    ];
    final executable = candidates
        .where((path) => File(path).existsSync())
        .firstOrNull;
    if (executable == null) {
      throw CoreRpcException(
        'retcon-core.exe was not found. Build the Rust workspace or set RETCON_CORE_PATH.',
      );
    }
    await Process.start(executable, [
      '--data-dir',
      dataDirectory.path,
      '--log-format',
      'json',
    ], mode: ProcessStartMode.detached);
  }

  void _setStatus(CoreConnectionStatus value) {
    if (_status == value) return;
    _status = value;
    notifyListeners();
  }

  @override
  void dispose() {
    _closing = true;
    _heartbeat?.cancel();
    _lines?.cancel();
    _socket?.destroy();
    _events.close();
    super.dispose();
  }

  static Directory _defaultDataDirectory() {
    final base =
        Platform.environment['LOCALAPPDATA'] ?? Directory.systemTemp.path;
    return Directory('$base${Platform.pathSeparator}Retcon');
  }
}
