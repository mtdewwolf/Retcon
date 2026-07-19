import 'dart:async';
import 'dart:collection';
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

/// A bounded, local record of Core process and connection output.
class CoreLogEntry {
  const CoreLogEntry({
    required this.timestamp,
    required this.source,
    required this.message,
  });

  final DateTime timestamp;
  final String source;
  final String message;
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
  final _coreLogs = StreamController<CoreLogEntry>.broadcast();
  final _pending = <int, Completer<Map<String, dynamic>>>{};
  final _recentCoreLogs = ListQueue<CoreLogEntry>();
  final _coreOutputSubscriptions = <StreamSubscription<String>>[];
  ProtocolConnection? _connection;
  Future<void> _writeTail = Future.value();
  StreamSubscription<String>? _lines;
  Timer? _heartbeat;
  Timer? _reconnectTimer;
  bool _closing = false;
  int _nextId = 0;
  int _generation = 0;
  int _reconnectAttempt = 0;
  CoreConnectionStatus _status = CoreConnectionStatus.disconnected;
  Object? _lastConnectionError;

  CoreConnectionStatus get status => _status;
  Object? get lastConnectionError => _lastConnectionError;
  Stream<Map<String, dynamic>> get events => _events.stream;
  Stream<CoreRequestMetric> get requestMetrics => _requestMetrics.stream;
  Stream<CoreLogEntry> get coreLogs => _coreLogs.stream;
  List<CoreLogEntry> get recentCoreLogs => List.unmodifiable(_recentCoreLogs);

  void clearCoreLogs() {
    _recentCoreLogs.clear();
    notifyListeners();
  }

  Future<void> connect({bool launchIfNeeded = true}) async {
    if (_status == CoreConnectionStatus.connected ||
        _status == CoreConnectionStatus.connecting) {
      return;
    }
    _reconnectTimer?.cancel();
    _reconnectTimer = null;
    _setStatus(
      _generation == 0
          ? CoreConnectionStatus.connecting
          : CoreConnectionStatus.reconnecting,
    );
    _closing = false;
    try {
      _recordCoreLog('client', 'Connecting to Retcon Core.');
      var discovery = await _readDiscovery();
      if (discovery != null && !await _isProcessAlive(discovery.pid)) {
        _warn(
          'stale core discovery removed',
          'pid=${discovery.pid} path=${discovery.path}',
        );
        await _removeDiscovery();
        discovery = null;
      }
      if (discovery == null && launchIfNeeded) {
        await _launchCore();
        discovery = await _waitForDiscovery();
      }
      if (discovery == null) {
        throw CoreRpcException('Retcon Core is not running.');
      }
      _recordCoreLog(
        'client',
        'Using Core discovery for PID ${discovery.pid} at ${discovery.path}.',
      );
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
          _writeLine(
            jsonEncode(const PingFrame().toJson()),
          ).catchError((Object _) {}),
        );
        request(
          'core.health',
          timeout: const Duration(seconds: 3),
        ).catchError((Object error) => <String, dynamic>{});
      });
      await request('core.health');
      _lastConnectionError = null;
      _reconnectAttempt = 0;
      _recordCoreLog(
        'client',
        'Connected to Retcon Core (PID ${discovery.pid}).',
      );
    } catch (error) {
      _lastConnectionError = error;
      _setStatus(CoreConnectionStatus.disconnected);
      _warn('Core connection failed', error);
      if (!_closing && launchIfNeeded) {
        _scheduleReconnect();
      }
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
      await _writeLine(
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
    if (_connection == null) return;
    await _writeLine(jsonEncode(CancelFrame(id).toJson()));
  }

  Future<void> _writeLine(String line) {
    final connection = _connection;
    if (connection == null) {
      return Future<void>.error(
        CoreRpcException('Retcon Core is disconnected.'),
      );
    }
    final write = _writeTail.then((_) => connection.writeLine(line));
    _writeTail = write.then<void>((_) {}, onError: (_, _) {});
    return write;
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
    if (_closing || _reconnectTimer != null) return;
    _warn('core connection closed', error);
    _connection = null;
    _writeTail = Future.value();
    for (final pending in _pending.values) {
      if (!pending.isCompleted) {
        pending.completeError(CoreRpcException('Core connection closed.'));
      }
    }
    _pending.clear();
    _scheduleReconnect();
  }

  void _scheduleReconnect() {
    if (_closing || _reconnectTimer != null) return;
    _setStatus(CoreConnectionStatus.reconnecting);
    _reconnectAttempt++;
    final delay = _reconnectAttempt == 1
        ? const Duration(milliseconds: 500)
        : const Duration(seconds: 2);
    _reconnectTimer = Timer(delay, () {
      _reconnectTimer = null;
      if (_closing) return;
      unawaited(_retryConnection());
    });
  }

  Future<void> _retryConnection() async {
    try {
      await connect();
    } catch (error) {
      if (!_closing) {
        _warn('core reconnect attempt failed', error);
        _scheduleReconnect();
      }
    }
  }

  Future<Discovery?> _readDiscovery() async {
    final file = _discoveryFile;
    if (!await file.exists()) return null;
    try {
      return Discovery.fromJson(
        (jsonDecode(await file.readAsString()) as Map).cast<String, dynamic>(),
      );
    } catch (error) {
      _warn('invalid core discovery removed', error);
      await _removeDiscovery();
      return null;
    }
  }

  File get _discoveryFile =>
      File('${dataDirectory.path}${Platform.pathSeparator}core.json');

  Future<void> _removeDiscovery() async {
    try {
      if (await _discoveryFile.exists()) {
        await _discoveryFile.delete();
      }
    } catch (error) {
      _warn('could not remove stale core discovery', error);
    }
  }

  Future<bool> _isProcessAlive(int pid) async {
    if (pid <= 0) return false;
    try {
      if (Platform.isWindows) {
        final result = await Process.run('tasklist', [
          '/FI',
          'PID eq $pid',
          '/FO',
          'CSV',
          '/NH',
        ]);
        if (result.exitCode != 0) return true;
        final output = result.stdout.toString();
        return RegExp(
          r'^"retcon-core\.exe","' + pid.toString() + r'",',
          caseSensitive: false,
          multiLine: true,
        ).hasMatch(output);
      }
      final result = await Process.run('kill', ['-0', pid.toString()]);
      return result.exitCode == 0;
    } catch (error) {
      // Failing to inspect the PID should never make us delete a possibly
      // active core discovery record. Keep it and let the connection retry.
      _warn('could not validate core process', error);
      return true;
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
      ..._coreExecutableCandidates(),
    ];
    final executable = candidates
        .where((path) => File(path).existsSync())
        .firstOrNull;
    if (executable == null) {
      _recordCoreLog(
        'launcher',
        'retcon-core.exe was not found in any configured launch location.',
      );
      throw CoreRpcException(
        'retcon-core.exe was not found. Build the Rust workspace or set RETCON_CORE_PATH.',
      );
    }
    final process = await Process.start(executable, [
      '--data-dir',
      dataDirectory.path,
      '--log-format',
      'json',
    ], mode: ProcessStartMode.detachedWithStdio);
    _recordCoreLog('launcher', 'Started retcon-core.exe (PID ${process.pid}).');
    _captureCoreOutput(process.stdout, 'core stdout');
    _captureCoreOutput(process.stderr, 'core stderr');
  }

  Iterable<String> _coreExecutableCandidates() sync* {
    var directory = File(Platform.resolvedExecutable).parent;
    for (var depth = 0; depth < 8; depth++) {
      yield '${directory.path}${Platform.pathSeparator}retcon-core.exe';
      yield '${directory.path}${Platform.pathSeparator}target${Platform.pathSeparator}debug${Platform.pathSeparator}retcon-core.exe';
      yield '${directory.path}${Platform.pathSeparator}target${Platform.pathSeparator}release${Platform.pathSeparator}retcon-core.exe';
      final parent = directory.parent;
      if (parent.path == directory.path) return;
      directory = parent;
    }
  }

  void _captureCoreOutput(Stream<List<int>> output, String source) {
    final subscription = output
        .transform(utf8.decoder)
        .transform(const LineSplitter())
        .listen(
          (line) => _recordCoreLog(source, line),
          onError: (Object error) => _warn('could not read $source', error),
        );
    _coreOutputSubscriptions.add(subscription);
  }

  void _warn(String message, [Object? error]) {
    _log.warning(message, error);
    _recordCoreLog('client', error == null ? message : '$message: $error');
  }

  void _recordCoreLog(String source, String message) {
    final normalized = message.trim();
    if (normalized.isEmpty) return;
    final entry = CoreLogEntry(
      timestamp: DateTime.now().toLocal(),
      source: source,
      message: normalized,
    );
    _recentCoreLogs.add(entry);
    while (_recentCoreLogs.length > 512) {
      _recentCoreLogs.removeFirst();
    }
    if (!_coreLogs.isClosed) _coreLogs.add(entry);
    notifyListeners();
  }

  void _setStatus(CoreConnectionStatus value) {
    if (_status == value) return;
    _status = value;
    notifyListeners();
  }

  @override
  void dispose() {
    _closing = true;
    _reconnectTimer?.cancel();
    _heartbeat?.cancel();
    _lines?.cancel();
    for (final subscription in _coreOutputSubscriptions) {
      unawaited(subscription.cancel());
    }
    unawaited(_connection?.close());
    _events.close();
    _requestMetrics.close();
    _coreLogs.close();
    super.dispose();
  }

  static Directory _defaultDataDirectory() {
    final base =
        Platform.environment['LOCALAPPDATA'] ?? Directory.systemTemp.path;
    return Directory('$base${Platform.pathSeparator}Retcon');
  }
}
