import 'dart:async';
import 'dart:collection';
import 'dart:ui';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:logging/logging.dart';

import '../core_client.dart';
import 'core_diagnostics_repository.dart';
import 'diagnostics_models.dart';
import 'diagnostics_repository.dart';

class DesktopDiagnostics {
  DesktopDiagnostics();

  static final instance = DesktopDiagnostics();
  static const maxBufferedLogs = 512;
  static const maxBufferedMetrics = 512;
  static const maxBatchSize = 64;

  final ListQueue<DiagnosticIngestRecord> _logs = ListQueue();
  final ListQueue<DiagnosticMetricRecord> _metrics = ListQueue();
  DiagnosticsRepository? _repository;
  CoreClient? _core;
  StreamSubscription<CoreRequestMetric>? _requestMetrics;
  Timer? _flushTimer;
  bool _telemetryEnabled = false;
  bool _timingsInstalled = false;
  bool _flushing = false;
  bool _syncingPrivacy = false;

  int get bufferedLogCount => _logs.length;
  int get bufferedMetricCount => _metrics.length;
  bool get telemetryEnabled => _telemetryEnabled;

  void bindCore(CoreClient core, {DiagnosticsRepository? repository}) {
    unawaited(_requestMetrics?.cancel());
    _core?.removeListener(_handleCoreState);
    _core = core;
    _repository = repository ?? CoreDiagnosticsRepository.fromCore(core);
    _requestMetrics = core.requestMetrics.listen(_captureRequestMetric);
    core.addListener(_handleCoreState);
    _handleCoreState();
  }

  void captureLog(LogRecord record) {
    _addLog(
      DiagnosticIngestRecord(
        timestamp: record.time.toUtc(),
        component: _safeComponent(record.loggerName),
        severity: _severity(record.level),
        code: record.level >= Level.SEVERE
            ? 'desktop_log_error'
            : 'desktop_log',
      ),
    );
  }

  void captureFlutterError() {
    _addLog(
      DiagnosticIngestRecord(
        timestamp: DateTime.now().toUtc(),
        component: 'flutter',
        severity: DiagnosticSeverity.error,
        code: 'flutter_error',
      ),
    );
  }

  void capturePlatformError() {
    _addLog(
      DiagnosticIngestRecord(
        timestamp: DateTime.now().toUtc(),
        component: 'platform_dispatcher',
        severity: DiagnosticSeverity.error,
        code: 'platform_error',
      ),
    );
  }

  void captureOperationFailure({
    required String component,
    required String code,
  }) {
    _addLog(
      DiagnosticIngestRecord(
        timestamp: DateTime.now().toUtc(),
        component: _safeComponent(component),
        severity: DiagnosticSeverity.error,
        code: _safeCode(code),
      ),
    );
  }

  void setTelemetryEnabled(bool enabled) {
    _telemetryEnabled = enabled;
    if (enabled && !_timingsInstalled) {
      WidgetsBinding.instance.addTimingsCallback(_captureFrameTimings);
      _timingsInstalled = true;
    } else if (!enabled && _timingsInstalled) {
      WidgetsBinding.instance.removeTimingsCallback(_captureFrameTimings);
      _timingsInstalled = false;
      _metrics.clear();
    }
  }

  void clearBufferedData() {
    _logs.clear();
    _metrics.clear();
  }

  Future<void> flush() async {
    final repository = _repository;
    if (repository == null || _flushing) return;
    _flushing = true;
    final logs = _take(_logs, maxBatchSize);
    final metrics = _telemetryEnabled
        ? _take(_metrics, maxBatchSize)
        : <DiagnosticMetricRecord>[];
    try {
      await repository.ingestLogs(logs);
      await repository.ingestMetrics(metrics);
    } on Object {
      _restore(_logs, logs, maxBufferedLogs);
      _restore(_metrics, metrics, maxBufferedMetrics);
    } finally {
      _flushing = false;
    }
  }

  Future<void> dispose() async {
    _flushTimer?.cancel();
    _flushTimer = null;
    _core?.removeListener(_handleCoreState);
    _core = null;
    unawaited(_requestMetrics?.cancel());
    _requestMetrics = null;
    if (_timingsInstalled) {
      WidgetsBinding.instance.removeTimingsCallback(_captureFrameTimings);
      _timingsInstalled = false;
    }
  }

  void _handleCoreState() {
    if (_core?.status != CoreConnectionStatus.connected) {
      _flushTimer?.cancel();
      _flushTimer = null;
      return;
    }
    _flushTimer ??= Timer.periodic(
      const Duration(seconds: 2),
      (_) => unawaited(flush()),
    );
    if (_syncingPrivacy) {
      return;
    }
    _syncingPrivacy = true;
    final repository = _repository;
    if (repository == null) {
      _syncingPrivacy = false;
      return;
    }
    unawaited(
      repository
          .loadPrivacy()
          .then((privacy) => setTelemetryEnabled(privacy.telemetryEnabled))
          .catchError((Object _) {})
          .whenComplete(() => _syncingPrivacy = false),
    );
  }

  void _captureRequestMetric(CoreRequestMetric metric) {
    if (!_telemetryEnabled) return;
    _addMetric(
      DiagnosticMetricRecord(
        timestamp: metric.timestamp,
        name: 'ipc.duration',
        value: metric.duration.inMicroseconds / 1000,
        dimensions: {
          'method': _safeMethod(metric.method),
          'outcome': _safeOutcome(metric.outcome),
        },
      ),
    );
  }

  void _captureFrameTimings(List<FrameTiming> timings) {
    if (!_telemetryEnabled) return;
    for (final timing in timings.take(maxBatchSize)) {
      _addMetric(
        DiagnosticMetricRecord(
          timestamp: DateTime.now().toUtc(),
          name: 'ui.frame.duration',
          value: timing.totalSpan.inMicroseconds / 1000,
          dimensions: {
            'jank': timing.totalSpan > const Duration(milliseconds: 16)
                ? 'true'
                : 'false',
          },
        ),
      );
    }
  }

  void _addLog(DiagnosticIngestRecord record) {
    if (_logs.length >= maxBufferedLogs) _logs.removeFirst();
    _logs.addLast(record);
  }

  void _addMetric(DiagnosticMetricRecord record) {
    if (_metrics.length >= maxBufferedMetrics) _metrics.removeFirst();
    _metrics.addLast(record);
  }
}

void installDesktopErrorCapture() {
  final previousFlutter = FlutterError.onError;
  FlutterError.onError = (details) {
    DesktopDiagnostics.instance.captureFlutterError();
    if (previousFlutter != null) {
      previousFlutter(details);
    } else {
      FlutterError.presentError(details);
    }
  };
  final previousPlatform = PlatformDispatcher.instance.onError;
  PlatformDispatcher.instance.onError = (error, stack) {
    DesktopDiagnostics.instance.capturePlatformError();
    return previousPlatform?.call(error, stack) ?? false;
  };
}

List<T> _take<T>(ListQueue<T> queue, int limit) {
  final values = <T>[];
  while (queue.isNotEmpty && values.length < limit) {
    values.add(queue.removeFirst());
  }
  return values;
}

void _restore<T>(ListQueue<T> queue, List<T> values, int limit) {
  for (final value in values.reversed) {
    queue.addFirst(value);
  }
  while (queue.length > limit) {
    queue.removeLast();
  }
}

DiagnosticSeverity _severity(Level level) {
  if (level >= Level.SHOUT) return DiagnosticSeverity.critical;
  if (level >= Level.SEVERE) return DiagnosticSeverity.error;
  if (level >= Level.WARNING) return DiagnosticSeverity.warning;
  return DiagnosticSeverity.info;
}

String _safeComponent(String value) =>
    RegExp(r'^[A-Za-z0-9_.-]{1,64}$').hasMatch(value) ? value : 'desktop';

String _safeMethod(String value) =>
    RegExp(r'^[A-Za-z0-9_.-]{1,96}$').hasMatch(value) ? value : 'unknown';

String _safeOutcome(String value) => switch (value) {
  'success' || 'error' || 'timeout' || 'disconnected' => value,
  _ => 'error',
};

String _safeCode(String value) =>
    RegExp(r'^[a-z0-9_]{1,64}$').hasMatch(value) ? value : 'operation_failed';
