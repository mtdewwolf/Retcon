enum DiagnosticSeverity { info, warning, error, critical }

class DiagnosticOverview {
  const DiagnosticOverview({
    required this.coreStatus,
    required this.version,
    required this.uptime,
    required this.storageBytes,
  });

  final String coreStatus;
  final String version;
  final Duration uptime;
  final int storageBytes;
}

class DiagnosticDistribution {
  const DiagnosticDistribution({
    required this.count,
    required this.p50Ms,
    required this.p95Ms,
    required this.maxMs,
    this.jankCount = 0,
  });

  final int count;
  final double p50Ms;
  final double p95Ms;
  final double maxMs;
  final int jankCount;
}

class DiagnosticError {
  const DiagnosticError({
    required this.id,
    required this.timestamp,
    required this.component,
    required this.code,
    required this.severity,
    required this.message,
  });

  final String id;
  final DateTime timestamp;
  final String component;
  final String code;
  final DiagnosticSeverity severity;
  final String message;
}

class DiagnosticOwnedResource {
  const DiagnosticOwnedResource({
    required this.id,
    required this.kind,
    required this.status,
    this.startedAt,
    this.port,
  });

  final String id;
  final String kind;
  final String status;
  final DateTime? startedAt;
  final int? port;
}

class DiagnosticFieldDocumentation {
  const DiagnosticFieldDocumentation({
    required this.name,
    required this.purpose,
    required this.retention,
  });

  final String name;
  final String purpose;
  final String retention;
}

class DiagnosticPrivacy {
  const DiagnosticPrivacy({
    required this.telemetryEnabled,
    required this.retentionDays,
    required this.fields,
  });

  final bool telemetryEnabled;
  final int retentionDays;
  final List<DiagnosticFieldDocumentation> fields;

  DiagnosticPrivacy copyWith({bool? telemetryEnabled}) => DiagnosticPrivacy(
    telemetryEnabled: telemetryEnabled ?? this.telemetryEnabled,
    retentionDays: retentionDays,
    fields: fields,
  );
}

class DiagnosticsSnapshot {
  const DiagnosticsSnapshot({
    required this.overview,
    required this.ipc,
    required this.uiFrames,
    required this.errors,
    required this.processes,
    required this.sessions,
    required this.ports,
    required this.privacy,
  });

  final DiagnosticOverview overview;
  final DiagnosticDistribution ipc;
  final DiagnosticDistribution uiFrames;
  final List<DiagnosticError> errors;
  final List<DiagnosticOwnedResource> processes;
  final List<DiagnosticOwnedResource> sessions;
  final List<DiagnosticOwnedResource> ports;
  final DiagnosticPrivacy privacy;
}

class SupportBundleReceipt {
  const SupportBundleReceipt({
    required this.id,
    required this.fileName,
    required this.sizeBytes,
    required this.createdAt,
    required this.content,
  });

  final String id;
  final String fileName;
  final int sizeBytes;
  final DateTime createdAt;
  final String content;
}

class DiagnosticIngestRecord {
  const DiagnosticIngestRecord({
    required this.timestamp,
    required this.component,
    required this.severity,
    required this.code,
    this.count = 1,
  });

  final DateTime timestamp;
  final String component;
  final DiagnosticSeverity severity;
  final String code;
  final int count;

  Map<String, dynamic> toWire() => {
    'timestamp': timestamp.toUtc().toIso8601String(),
    'component': component,
    'severity': severity.name,
    'code': code,
    'count': count,
  };
}

class DiagnosticMetricRecord {
  const DiagnosticMetricRecord({
    required this.timestamp,
    required this.name,
    required this.value,
    this.dimensions = const {},
  });

  final DateTime timestamp;
  final String name;
  final double value;
  final Map<String, String> dimensions;

  Map<String, dynamic> toWire() => {
    'timestamp': timestamp.toUtc().toIso8601String(),
    'name': name,
    'value': value,
    if (dimensions.isNotEmpty) 'dimensions': dimensions,
  };
}
