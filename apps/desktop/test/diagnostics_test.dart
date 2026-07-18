import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:logging/logging.dart';
import 'package:retcon_desktop/src/core_client.dart';
import 'package:retcon_desktop/src/diagnostics/diagnostics.dart';
import 'package:retcon_desktop/src/provider_doctor_dialog.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

void main() {
  group('CoreDiagnosticsRepository', () {
    test(
      'emits the exact v11 contract and decodes bounded safe DTOs',
      () async {
        final rpc = _DiagnosticsRpc();
        final repository = CoreDiagnosticsRepository(rpc);

        final snapshot = await repository.load();

        expect(rpc.calls.take(5).map((call) => call.method), [
          'diagnostics.snapshot',
          'diagnostics.logs.list',
          'diagnostics.metrics.query',
          'diagnostics.privacy.get',
          'diagnostics.fields',
        ]);
        expect(rpc.calls[1].params, {'limit': 50, 'severity': 'error'});
        expect(rpc.calls[2].params, {'window': 'session'});
        expect(snapshot.errors.single.message, contains('redacted'));
        expect(snapshot.processes.single.id, processId);
        expect(snapshot.overview.coreStatus, 'healthy');
        expect(snapshot.overview.storageBytes, 3072);
        expect(snapshot.privacy.telemetryEnabled, isFalse);
        expect(snapshot.privacy.fields.single.name, 'durationMs');

        await repository.ingestLogs([
          DiagnosticIngestRecord(
            timestamp: DateTime.utc(2026),
            component: 'desktop',
            severity: DiagnosticSeverity.error,
            code: 'flutter_error',
          ),
        ]);
        await repository.ingestMetrics([
          DiagnosticMetricRecord(
            timestamp: DateTime.utc(2026),
            name: 'ipc.duration',
            value: 12.5,
            dimensions: const {'method': 'core.health', 'outcome': 'success'},
          ),
        ]);
        await repository.setTelemetry(true);
        final bundle = await repository.exportSupportBundle();
        await repository.exportSupportBundle(context: 'provider_doctor');
        final deleted = await repository.deleteDiagnosticData();

        final logCall = rpc.calls.firstWhere(
          (call) => call.method == 'diagnostics.logs.ingest',
        );
        expect((logCall.params['records'] as List).single, {
          'timestamp': '2026-01-01T00:00:00.000Z',
          'component': 'desktop',
          'severity': 'error',
          'code': 'flutter_error',
          'count': 1,
        });
        expect(
          rpc.calls
              .firstWhere((call) => call.method == 'diagnostics.metrics.ingest')
              .params,
          contains('metrics'),
        );
        expect(
          rpc.calls
              .firstWhere((call) => call.method == 'diagnostics.privacy.update')
              .params,
          {'telemetryEnabled': true},
        );
        expect(
          rpc.calls
              .firstWhere(
                (call) => call.method == 'diagnostics.supportBundle.create',
              )
              .params,
          {
            'includeLogs': true,
            'includeMetrics': true,
            'includeRecentErrors': true,
            'context': 'diagnostics',
          },
        );
        expect(
          rpc.calls
              .firstWhere((call) => call.method == 'diagnostics.data.delete')
              .params,
          {'scope': 'diagnostics', 'confirmation': 'delete'},
        );
        expect(bundle.fileName, 'retcon-support.zip');
        expect(bundle.content, contains('formatVersion'));
        expect(deleted['logs'], 3);
        expect(
          rpc.calls
              .where(
                (call) => call.method == 'diagnostics.supportBundle.create',
              )
              .last
              .params['context'],
          'provider_doctor',
        );
      },
    );
  });

  testWidgets(
    'diagnostics workspace exposes privacy, errors, and safe actions',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1050, 780));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final repository = _MemoryDiagnosticsRepository();
      final runtime = DesktopDiagnostics();
      final controller = DiagnosticsController(
        repository: repository,
        desktopDiagnostics: runtime,
      );
      addTearDown(() async {
        controller.dispose();
        await runtime.dispose();
      });
      await controller.load();

      await tester.pumpWidget(_app(DiagnosticsPanel(controller: controller)));
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('diagnostics-overview')), findsOneWidget);
      expect(
        find.text('Optional performance telemetry is off.'),
        findsOneWidget,
      );
      expect(runtime.telemetryEnabled, isFalse);

      await tester.tap(find.text('Recent Errors'));
      await tester.pumpAndSettle();
      expect(
        find.byKey(const Key('diagnostics-recent-errors')),
        findsOneWidget,
      );
      expect(find.textContaining('render_failed'), findsOneWidget);

      await tester.tap(find.text('Owned Resources'));
      await tester.pumpAndSettle();
      expect(find.text('Processes (1)'), findsOneWidget);
      expect(find.text('Ports (1)'), findsOneWidget);

      await tester.tap(find.text('Privacy & Data'));
      await tester.pumpAndSettle();
      expect(find.textContaining('Prompts, terminal output'), findsOneWidget);
      await tester.tap(find.byKey(const Key('diagnostics-telemetry-toggle')));
      await tester.pumpAndSettle();
      expect(repository.telemetryEnabled, isTrue);
      expect(runtime.telemetryEnabled, isTrue);

      await tester.tap(find.byKey(const Key('delete-diagnostic-data')));
      await tester.pumpAndSettle();
      expect(find.text('Delete diagnostic data?'), findsOneWidget);
      await tester.tap(find.byKey(const Key('confirm-delete-diagnostic-data')));
      await tester.pumpAndSettle();
      expect(repository.deleted, isTrue);

      await tester.tap(find.text('Support Bundle'));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const Key('export-support-bundle')));
      await tester.pumpAndSettle();
      expect(find.text('retcon-support.zip'), findsOneWidget);
      expect(find.textContaining(r'C:\'), findsNothing);
    },
  );

  testWidgets('diagnostics remains usable at minimum panel width', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(620, 620));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final runtime = DesktopDiagnostics();
    final controller = DiagnosticsController(
      repository: _MemoryDiagnosticsRepository(),
      desktopDiagnostics: runtime,
    );
    addTearDown(() async {
      controller.dispose();
      await runtime.dispose();
    });
    await controller.load();
    await tester.pumpWidget(_app(DiagnosticsPanel(controller: controller)));
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
    await tester.ensureVisible(find.text('Privacy & Data'));
    await tester.tap(find.text('Privacy & Data'));
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
  });

  testWidgets('structured buffering excludes content and gates metrics', (
    tester,
  ) async {
    final repository = _MemoryDiagnosticsRepository();
    final core = _MetricCoreClient();
    final runtime = DesktopDiagnostics()
      ..bindCore(core, repository: repository);

    runtime.captureLog(
      LogRecord(
        Level.SEVERE,
        'prompt=secret terminal command C:\\private\\file.txt',
        'unsafe/path',
      ),
    );
    core.emitMetric('task.get');
    await tester.pump();
    await runtime.flush();
    expect(repository.logs.single.code, 'desktop_log_error');
    expect(repository.logs.single.component, 'desktop');
    expect(repository.metrics, isEmpty);

    runtime.setTelemetryEnabled(true);
    core.emitMetric('task.get');
    await tester.pump();
    await runtime.flush();
    await runtime.dispose();
    core.dispose();
    expect(repository.metrics.single.name, 'ipc.duration');
    expect(repository.metrics.single.dimensions['method'], 'task.get');
  });

  test('CoreClient excludes diagnostics RPCs from roundtrip metrics', () async {
    final core = CoreClient(dataDirectory: Directory.systemTemp);
    final metrics = <CoreRequestMetric>[];
    final subscription = core.requestMetrics.listen(metrics.add);
    addTearDown(() async {
      await subscription.cancel();
      core.dispose();
    });

    await expectLater(
      core.request('diagnostics.snapshot'),
      throwsA(isA<CoreRpcException>()),
    );
    await expectLater(
      core.request('task.get'),
      throwsA(isA<CoreRpcException>()),
    );
    await Future<void>.delayed(Duration.zero);

    expect(metrics, hasLength(1));
    expect(metrics.single.method, 'task.get');
    expect(metrics.single.outcome, 'disconnected');
  });

  testWidgets('persisted Core opt-in is synchronized after connection', (
    tester,
  ) async {
    final repository = _MemoryDiagnosticsRepository()..telemetryEnabled = true;
    final core = _MetricCoreClient(connected: true);
    final runtime = DesktopDiagnostics()
      ..bindCore(core, repository: repository);

    await tester.pump();
    await runtime.dispose();
    core.dispose();

    expect(runtime.telemetryEnabled, isTrue);
  });

  test('controller never surfaces exception payloads', () async {
    final runtime = DesktopDiagnostics();
    final controller = DiagnosticsController(
      repository: _FailingDiagnosticsRepository(),
      desktopDiagnostics: runtime,
    );
    addTearDown(() async {
      controller.dispose();
      await runtime.dispose();
    });

    await controller.load();

    expect(
      controller.error,
      'Diagnostics are temporarily unavailable. Try again.',
    );
    expect(controller.error, isNot(contains(r'C:\Users\private')));
    expect(controller.error, isNot(contains('token=secret')));
  });

  testWidgets('provider doctor never surfaces transport exception payloads', (
    tester,
  ) async {
    final core = _FailingCoreClient();
    await tester.pumpWidget(_app(ProviderDoctorDialog(core: core)));
    await tester.pumpAndSettle();

    expect(
      find.textContaining('Provider checks are temporarily unavailable.'),
      findsOneWidget,
    );
    expect(find.textContaining(r'C:\Users\private'), findsNothing);
    expect(find.textContaining('token=secret'), findsNothing);
    core.dispose();
  });
}

Widget _app(Widget child) => MaterialApp(
  theme: buildLunaDarkTheme(),
  home: Scaffold(body: child),
);

class _RpcCall {
  const _RpcCall(this.method, this.params);
  final String method;
  final Map<String, dynamic> params;
}

class _DiagnosticsRpc implements DiagnosticsRpcClient {
  final calls = <_RpcCall>[];

  @override
  Future<Map<String, dynamic>> request(
    String method, {
    Map<String, dynamic> params = const {},
  }) async {
    calls.add(_RpcCall(method, params));
    return switch (method) {
      'diagnostics.snapshot' => snapshotWire,
      'diagnostics.logs.list' => {
        'logs': [errorWire],
      },
      'diagnostics.metrics.query' => {'performance': performanceWire},
      'diagnostics.privacy.get' || 'diagnostics.privacy.update' => {
        'privacy': {
          ...privacyWire,
          if (method == 'diagnostics.privacy.update')
            'telemetryEnabled': params['telemetryEnabled'],
        },
      },
      'diagnostics.fields' => {
        'fields': [fieldWire],
      },
      'diagnostics.supportBundle.create' => {'bundle': bundleWire},
      'diagnostics.data.delete' => {
        'deleted': {'logs': 3, 'metrics': 2, 'errors': 1, 'bundles': 1},
      },
      _ => const {},
    };
  }
}

class _MemoryDiagnosticsRepository implements DiagnosticsRepository {
  bool telemetryEnabled = false;
  bool deleted = false;
  final logs = <DiagnosticIngestRecord>[];
  final metrics = <DiagnosticMetricRecord>[];

  @override
  Future<DiagnosticsSnapshot> load({int recentErrorLimit = 50}) async =>
      DiagnosticsDtoCodec.decodeSnapshot(
        snapshotWire,
        logs: [errorWire],
        performance: performanceWire,
        privacy: {...privacyWire, 'telemetryEnabled': telemetryEnabled},
        fields: [fieldWire],
      );

  @override
  Future<DiagnosticPrivacy> loadPrivacy() async =>
      DiagnosticsDtoCodec.decodePrivacy({
        ...privacyWire,
        'telemetryEnabled': telemetryEnabled,
        'fields': [fieldWire],
      });

  @override
  Future<void> ingestLogs(List<DiagnosticIngestRecord> records) async =>
      logs.addAll(records);

  @override
  Future<void> ingestMetrics(List<DiagnosticMetricRecord> values) async =>
      metrics.addAll(values);

  @override
  Future<DiagnosticPrivacy> setTelemetry(bool enabled) async {
    telemetryEnabled = enabled;
    return DiagnosticsDtoCodec.decodePrivacy({
      ...privacyWire,
      'telemetryEnabled': enabled,
      'fields': [fieldWire],
    });
  }

  @override
  Future<SupportBundleReceipt> exportSupportBundle({
    String context = 'diagnostics',
  }) async => DiagnosticsDtoCodec.decodeBundle(bundleWire);

  @override
  Future<Map<String, int>> deleteDiagnosticData() async {
    deleted = true;
    logs.clear();
    metrics.clear();
    return {'logs': 3, 'metrics': 2};
  }
}

class _FailingDiagnosticsRepository extends _MemoryDiagnosticsRepository {
  @override
  Future<DiagnosticsSnapshot> load({int recentErrorLimit = 50}) =>
      Future.error(StateError(r'C:\Users\private\notes.txt token=secret'));
}

class _MetricCoreClient extends CoreClient {
  _MetricCoreClient({this.connected = false})
    : super(dataDirectory: Directory.systemTemp);
  final bool connected;
  final _metrics = StreamController<CoreRequestMetric>.broadcast();

  @override
  CoreConnectionStatus get status => connected
      ? CoreConnectionStatus.connected
      : CoreConnectionStatus.disconnected;

  @override
  Stream<CoreRequestMetric> get requestMetrics => _metrics.stream;

  void emitMetric(String method) => _metrics.add(
    CoreRequestMetric(
      method: method,
      duration: const Duration(milliseconds: 12),
      outcome: 'success',
      timestamp: DateTime.now().toUtc(),
    ),
  );

  @override
  void dispose() {
    _metrics.close();
    super.dispose();
  }
}

class _FailingCoreClient extends CoreClient {
  _FailingCoreClient() : super(dataDirectory: Directory.systemTemp);

  @override
  CoreConnectionStatus get status => CoreConnectionStatus.connected;

  @override
  Future<Map<String, dynamic>> request(
    String method, {
    Map<String, dynamic> params = const {},
    Duration timeout = const Duration(seconds: 30),
  }) =>
      Future.error(StateError(r'C:\Users\private\provider.json token=secret'));
}

const processId = 'process-1';

const snapshotWire = <String, dynamic>{
  'overview': {'status': 'healthy', 'version': '0.1.0', 'uptimeMs': 3600000},
  'disk': {'databaseBytes': 2048, 'artifactBytes': 1024},
  'resources': {
    'processes': [
      {'id': processId, 'kind': 'core', 'status': 'running'},
    ],
    'sessions': [
      {'id': 'session-1', 'kind': 'agent', 'status': 'active'},
    ],
    'ports': [
      {
        'id': 'port-5173',
        'ownerKind': 'dev_server',
        'status': 'bound',
        'port': 5173,
      },
    ],
  },
};

const performanceWire = <String, dynamic>{
  'ipc': {'count': 10, 'p50Ms': 3, 'p95Ms': 8, 'maxMs': 12},
  'uiFrames': {
    'count': 20,
    'p50Ms': 8,
    'p95Ms': 17,
    'maxMs': 24,
    'jankCount': 2,
  },
};

const privacyWire = <String, dynamic>{
  'telemetryEnabled': false,
  'retentionDays': 7,
};

const fieldWire = <String, dynamic>{
  'name': 'durationMs',
  'purpose': 'Measures operation latency.',
  'retention': '7 days',
};

const errorWire = <String, dynamic>{
  'id': 'error-1',
  'timestamp': '2026-07-17T12:00:00Z',
  'component': 'desktop',
  'code': 'render_failed',
  'severity': 'error',
  'message': r'Failed while reading C:\Users\private\secret.txt token=abc',
};

const bundleWire = <String, dynamic>{
  'id': 'bundle-1',
  'fileName': 'retcon-support.zip',
  'sizeBytes': 4096,
  'createdAt': '2026-07-17T12:01:00Z',
  'content': '{"formatVersion":1,"safe":true}',
};
