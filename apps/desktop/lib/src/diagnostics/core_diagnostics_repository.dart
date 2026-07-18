import '../core_client.dart';
import 'diagnostics_models.dart';
import 'diagnostics_repository.dart';

abstract interface class DiagnosticsRpcClient {
  Future<Map<String, dynamic>> request(
    String method, {
    Map<String, dynamic> params = const {},
  });
}

class CoreDiagnosticsRpcClient implements DiagnosticsRpcClient {
  CoreDiagnosticsRpcClient(this._core);

  final CoreClient _core;

  @override
  Future<Map<String, dynamic>> request(
    String method, {
    Map<String, dynamic> params = const {},
  }) => _core.request(method, params: params);
}

class CoreDiagnosticsRepository implements DiagnosticsRepository {
  CoreDiagnosticsRepository(this._rpc);

  factory CoreDiagnosticsRepository.fromCore(CoreClient core) =>
      CoreDiagnosticsRepository(CoreDiagnosticsRpcClient(core));

  final DiagnosticsRpcClient _rpc;

  @override
  Future<DiagnosticPrivacy> loadPrivacy() async {
    final response = await _rpc.request('diagnostics.privacy.get');
    return DiagnosticsDtoCodec.decodePrivacy(
      _map(response['privacy']).isEmpty ? response : _map(response['privacy']),
    );
  }

  @override
  Future<DiagnosticsSnapshot> load({int recentErrorLimit = 50}) async {
    final responses = await Future.wait([
      _rpc.request('diagnostics.snapshot'),
      _rpc.request(
        'diagnostics.logs.list',
        params: {'limit': recentErrorLimit.clamp(1, 100), 'severity': 'error'},
      ),
      _rpc.request(
        'diagnostics.metrics.query',
        params: const {'window': 'session'},
      ),
      _rpc.request('diagnostics.privacy.get'),
      _rpc.request('diagnostics.fields'),
    ]);
    return DiagnosticsDtoCodec.decodeSnapshot(
      _map(responses[0]['diagnostics']).isEmpty
          ? responses[0]
          : _map(responses[0]['diagnostics']),
      logs: _maps(responses[1]['logs']),
      performance: _map(responses[2]['performance'] ?? responses[2]['metrics']),
      privacy: _map(responses[3]['privacy']),
      fields: _maps(responses[4]['fields']),
    );
  }

  @override
  Future<void> ingestLogs(List<DiagnosticIngestRecord> records) async {
    if (records.isEmpty) return;
    await _rpc.request(
      'diagnostics.logs.ingest',
      params: {
        'records': records.take(64).map((record) => record.toWire()).toList(),
      },
    );
  }

  @override
  Future<void> ingestMetrics(List<DiagnosticMetricRecord> metrics) async {
    if (metrics.isEmpty) return;
    await _rpc.request(
      'diagnostics.metrics.ingest',
      params: {
        'metrics': metrics.take(64).map((metric) => metric.toWire()).toList(),
      },
    );
  }

  @override
  Future<DiagnosticPrivacy> setTelemetry(bool enabled) async {
    final response = await _rpc.request(
      'diagnostics.privacy.update',
      params: {'telemetryEnabled': enabled},
    );
    return DiagnosticsDtoCodec.decodePrivacy(
      _map(response['privacy']).isEmpty ? response : _map(response['privacy']),
    );
  }

  @override
  Future<SupportBundleReceipt> exportSupportBundle({
    String context = 'diagnostics',
  }) async {
    final response = await _rpc.request(
      'diagnostics.supportBundle.create',
      params: {
        'includeLogs': true,
        'includeMetrics': true,
        'includeRecentErrors': true,
        'context': context == 'provider_doctor'
            ? 'provider_doctor'
            : 'diagnostics',
      },
    );
    return DiagnosticsDtoCodec.decodeBundle(
      _map(response['bundle']).isEmpty ? response : _map(response['bundle']),
    );
  }

  @override
  Future<Map<String, int>> deleteDiagnosticData() async {
    final response = await _rpc.request(
      'diagnostics.data.delete',
      params: const {'scope': 'diagnostics', 'confirmation': 'delete'},
    );
    return {
      for (final entry in _map(response['deleted']).entries)
        if (entry.value is num) entry.key: (entry.value as num).toInt(),
    };
  }
}

class DiagnosticsDtoCodec {
  const DiagnosticsDtoCodec._();

  static DiagnosticsSnapshot decodeSnapshot(
    Map<String, dynamic> wire, {
    List<Map<String, dynamic>> logs = const [],
    Map<String, dynamic> performance = const {},
    Map<String, dynamic> privacy = const {},
    List<Map<String, dynamic>> fields = const [],
  }) {
    final performanceWire = performance.isEmpty
        ? _map(wire['performance'])
        : performance;
    final resources = _map(wire['resources']);
    final overview = _map(wire['overview']);
    final disk = _map(wire['disk']);
    return DiagnosticsSnapshot(
      overview: decodeOverview({
        ...overview,
        if (!overview.containsKey('storageBytes') && disk.isNotEmpty)
          'storageBytes':
              _int(disk['databaseBytes'], 0) + _int(disk['artifactBytes'], 0),
      }),
      ipc: decodeDistribution(_map(performanceWire['ipc'])),
      uiFrames: decodeDistribution(
        _map(performanceWire['uiFrames'] ?? performanceWire['ui_frames']),
      ),
      errors:
          (logs.isEmpty
                  ? _maps(wire['recentErrors'] ?? wire['recent_errors'])
                  : logs)
              .take(100)
              .map(decodeError)
              .toList(),
      processes: _maps(
        resources['processes'],
      ).take(128).map((item) => decodeResource(item, 'process')).toList(),
      sessions: _maps(
        resources['sessions'],
      ).take(128).map((item) => decodeResource(item, 'session')).toList(),
      ports: _maps(
        resources['ports'],
      ).take(128).map((item) => decodeResource(item, 'port')).toList(),
      privacy: decodePrivacy({
        ...(privacy.isEmpty ? _map(wire['privacy']) : privacy),
        if (fields.isNotEmpty) 'fields': fields,
      }),
    );
  }

  static DiagnosticOverview decodeOverview(Map<String, dynamic> wire) =>
      DiagnosticOverview(
        coreStatus: _safeLabel(
          wire['coreStatus'] ?? wire['core_status'] ?? wire['status'],
          'unknown',
        ),
        version: _safeLabel(wire['version'], 'unknown'),
        uptime: Duration(milliseconds: _int(wire['uptimeMs'], 0)),
        storageBytes: _int(wire['storageBytes'] ?? wire['storage_bytes'], 0),
      );

  static DiagnosticDistribution decodeDistribution(Map<String, dynamic> wire) =>
      DiagnosticDistribution(
        count: _int(wire['count'], 0),
        p50Ms: _double(wire['p50Ms'] ?? wire['p50_ms'], 0),
        p95Ms: _double(wire['p95Ms'] ?? wire['p95_ms'], 0),
        maxMs: _double(wire['maxMs'] ?? wire['max_ms'], 0),
        jankCount: _int(wire['jankCount'] ?? wire['jank_count'], 0),
      );

  static DiagnosticError decodeError(Map<String, dynamic> wire) =>
      DiagnosticError(
        id: _safeIdentifier(wire['id']),
        timestamp: _date(wire['timestamp'] ?? wire['createdAt']),
        component: _safeLabel(wire['component'], 'retcon'),
        code: _safeLabel(wire['code'], 'diagnostic_error'),
        severity: _severity(wire['severity']),
        message: _safeMessage(wire['message']),
      );

  static DiagnosticOwnedResource decodeResource(
    Map<String, dynamic> wire,
    String fallbackKind,
  ) => DiagnosticOwnedResource(
    id: _safeIdentifier(wire['id']),
    kind: _safeLabel(wire['kind'] ?? wire['ownerKind'], fallbackKind),
    status: _safeLabel(wire['status'], 'unknown'),
    startedAt: wire['startedAt'] == null ? null : _date(wire['startedAt']),
    port: wire['port'] is num ? (wire['port'] as num).toInt() : null,
  );

  static DiagnosticPrivacy decodePrivacy(Map<String, dynamic> wire) =>
      DiagnosticPrivacy(
        telemetryEnabled: wire['telemetryEnabled'] == true,
        retentionDays: _int(wire['retentionDays'], 0).clamp(0, 365),
        fields: _maps(wire['fields'])
            .take(64)
            .map(
              (field) => DiagnosticFieldDocumentation(
                name: _safeLabel(field['name'], 'field'),
                purpose: _safeMessage(field['purpose']),
                retention: _safeLabel(field['retention'], 'not retained'),
              ),
            )
            .toList(),
      );

  static SupportBundleReceipt decodeBundle(Map<String, dynamic> wire) {
    final content = _safeBundleContent(wire['content']);
    if (content == null) {
      throw const FormatException('Support bundle content was unavailable.');
    }
    return SupportBundleReceipt(
      id: _safeIdentifier(wire['id']),
      fileName: _safeFileName(wire['fileName'] ?? wire['file_name']),
      sizeBytes: _int(wire['sizeBytes'] ?? wire['size_bytes'], 0),
      createdAt: _date(wire['createdAt'] ?? wire['created_at']),
      content: content,
    );
  }
}

Map<String, dynamic> _map(Object? value) =>
    value is Map ? value.cast<String, dynamic>() : <String, dynamic>{};

List<Map<String, dynamic>> _maps(Object? value) => (value as List? ?? const [])
    .whereType<Map>()
    .map((item) => item.cast<String, dynamic>())
    .toList();

int _int(Object? value, int fallback) => (value as num?)?.toInt() ?? fallback;
double _double(Object? value, double fallback) =>
    (value as num?)?.toDouble() ?? fallback;

DateTime _date(Object? value) {
  if (value is num) return DateTime.fromMillisecondsSinceEpoch(value.toInt());
  return value is String
      ? DateTime.tryParse(value) ?? DateTime.fromMillisecondsSinceEpoch(0)
      : DateTime.fromMillisecondsSinceEpoch(0);
}

DiagnosticSeverity _severity(Object? value) => switch (value?.toString()) {
  'critical' => DiagnosticSeverity.critical,
  'error' => DiagnosticSeverity.error,
  'warning' => DiagnosticSeverity.warning,
  _ => DiagnosticSeverity.info,
};

String _safeIdentifier(Object? value) {
  final text = value?.toString() ?? 'unknown';
  return RegExp(r'^[A-Za-z0-9_.:-]{1,128}$').hasMatch(text) ? text : 'redacted';
}

String _safeLabel(Object? value, String fallback) {
  final text = value?.toString().trim() ?? '';
  if (text.isEmpty ||
      text.length > 128 ||
      text.contains(RegExp(r'[\\/\r\n]'))) {
    return fallback;
  }
  return text;
}

String _safeMessage(Object? value) {
  final text = value?.toString().trim() ?? '';
  if (text.isEmpty) return 'No additional detail.';
  final sensitive = RegExp(
    r'(?:[A-Za-z]:\\|/Users/|/home/|Bearer\s|cookie|secret|token|api[_-]?key|password|authorization|\r|\n)',
    caseSensitive: false,
  );
  if (sensitive.hasMatch(text)) {
    return 'Details redacted by the desktop privacy filter.';
  }
  return text.length <= 512 ? text : '${text.substring(0, 509)}...';
}

String _safeFileName(Object? value) {
  final text = value?.toString() ?? 'retcon-support-bundle.zip';
  return RegExp(r'^[A-Za-z0-9_.-]{1,128}$').hasMatch(text)
      ? text
      : 'retcon-support-bundle.zip';
}

String? _safeBundleContent(Object? value) {
  if (value is! String || value.isEmpty || value.length > 4 * 1024 * 1024) {
    return null;
  }
  final unsafe = RegExp(
    r'(?:[A-Za-z]:\\|/Users/|/home/|Bearer\s+|ghp_[A-Za-z0-9]|sk-[A-Za-z0-9]|AKIA[A-Z0-9]|BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY)',
    caseSensitive: false,
  );
  return unsafe.hasMatch(value) ? null : value;
}
