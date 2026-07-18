import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:logging/logging.dart';

import 'generated/protocol_v1.dart';
import 'transport.dart';

final _log = Logger('retcon.desktop.core');

enum CoreConnectionStatus { disconnected, connecting, connected, reconnecting }

class CoreRequestMetric {
  const CoreRequestMetric({
    required this.method,
    required this.duration,
    required this.outcome,
    required this.timestamp,
  });

  final String method;
  final Duration duration;
  final String outcome;
  final DateTime timestamp;
}

class CoreRpcException implements Exception {
  CoreRpcException(this.message, [this.details]);
  final String message;
  final Object? details;
  @override
  String toString() => message;
}

/// Local protocol v1 client for retcon-core (generated wire types + transport).
class CoreClient extends ChangeNotifier {
  CoreClient({Directory? dataDirectory})
    : dataDirectory = dataDirectory ?? _defaultDataDirectory();

  final Directory dataDirectory;
  final _events = StreamController<Map<String, dynamic>>.broadcast();
  final _requestMetrics = StreamController<CoreRequestMetric>.broadcast();
  final _pending = <int, Completer<Map<String, dynamic>>>{};
  ProtocolConnection? _connection;
  StreamSubscription<String>? _lines;
  Timer? _heartbeat;
  bool _closing = false;
  bool _reconnectScheduled = false;
  int _nextId = 0;
  int _generation = 0;
  CoreConnectionStatus _status = CoreConnectionStatus.disconnected;
  Object? _lastConnectionError;

  CoreConnectionStatus get status => _status;
  Object? get lastConnectionError => _lastConnectionError;
  Stream<Map<String, dynamic>> get events => _events.stream;
  Stream<CoreRequestMetric> get requestMetrics => _requestMetrics.stream;

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
      final connection = await openTransport(discovery);
      await connection.writeLine(jsonEncode({'auth': discovery.token}));
      await connection.writeLine(
        jsonEncode(
          const ClientHello(
            clientVersion: '0.1.0',
            features: ['ping', 'events.replay', 'request.cancel'],
          ).toJson(),
        ),
      );
      final helloLine = await connection.lines.first;
      final hello = jsonDecode(helloLine) as Map<String, dynamic>;
      if (hello['kind'] != 'server.hello') {
        throw CoreRpcException('Retcon Core sent an invalid hello response.');
      }
      _connection = connection;
      _lines = connection.lines.listen(
        _handleLine,
        onError: _handleDisconnect,
        onDone: _handleDisconnect,
      );
      _generation++;
      _setStatus(CoreConnectionStatus.connected);
      _heartbeat?.cancel();
      _heartbeat = Timer.periodic(const Duration(seconds: 10), (_) {
        unawaited(
          connection
              .writeLine(jsonEncode(const PingFrame().toJson()))
              .catchError((Object _) {}),
        );
        request(
          'core.health',
          timeout: const Duration(seconds: 3),
        ).catchError((Object error) => <String, dynamic>{});
      });
      await request('core.health');
      _lastConnectionError = null;
    } catch (error) {
      _lastConnectionError = error;
      _setStatus(CoreConnectionStatus.disconnected);
      rethrow;
    }
  }

  Future<Map<String, dynamic>> storageStatus() => request('storage.status');

  Future<Map<String, dynamic>> storageRecover(String action) =>
      request('storage.recover', params: {'action': action});

  Future<Map<String, dynamic>> request(
    String method, {
    Map<String, dynamic> params = const {},
    Duration timeout = const Duration(seconds: 30),
  }) async {
    final stopwatch = Stopwatch()..start();
    var outcome = 'error';
    if (_connection == null) {
      _emitRequestMetric(method, stopwatch, 'disconnected');
      throw CoreRpcException('Retcon Core is disconnected.');
    }
    final id = ++_nextId;
    final completer = Completer<Map<String, dynamic>>();
    _pending[id] = completer;
    try {
      await _connection!.writeLine(
        jsonEncode(RpcRequest(id: id, method: method, params: params).toJson()),
      );
      final result = await completer.future.timeout(timeout);
      outcome = 'success';
      return result;
    } on TimeoutException {
      outcome = 'timeout';
      throw CoreRpcException('$method timed out after ${timeout.inSeconds}s.');
    } finally {
      _pending.remove(id);
      _emitRequestMetric(method, stopwatch, outcome);
    }
  }

  void _emitRequestMetric(String method, Stopwatch stopwatch, String outcome) {
    if (_requestMetrics.isClosed || method.startsWith('diagnostics.')) return;
    _requestMetrics.add(
      CoreRequestMetric(
        method: method,
        duration: stopwatch.elapsed,
        outcome: outcome,
        timestamp: DateTime.now().toUtc(),
      ),
    );
  }

  Future<void> cancelRequest(int id) async {
    await _connection?.writeLine(jsonEncode(CancelFrame(id).toJson()));
  }

  void _handleLine(String line) {
    final value = jsonDecode(line) as Map<String, dynamic>;
    if (value['kind'] == 'pong') return;
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
    _connection = null;
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

  Future<Discovery?> _readDiscovery() async {
    final file = File(
      '${dataDirectory.path}${Platform.pathSeparator}core.json',
    );
    if (!await file.exists()) return null;
    try {
      return Discovery.fromJson(
        (jsonDecode(await file.readAsString()) as Map).cast<String, dynamic>(),
      );
    } catch (_) {
      return null;
    }
  }

  Future<Discovery?> _waitForDiscovery() async {
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
    unawaited(_connection?.close());
    _events.close();
    _requestMetrics.close();
    super.dispose();
  }

  static Directory _defaultDataDirectory() {
    final base =
        Platform.environment['LOCALAPPDATA'] ?? Directory.systemTemp.path;
    return Directory('$base${Platform.pathSeparator}Retcon');
  }
}
