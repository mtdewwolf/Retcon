enum DevServerStatus {
  stopped,
  starting,
  running,
  stopping,
  crashed,
  startupFailed,
  portConflict,
}

class DevServerEnvironmentVariable {
  const DevServerEnvironmentVariable({
    required this.key,
    required this.value,
    this.secret = false,
  });

  final String key;
  final String value;
  final bool secret;

  DevServerEnvironmentVariable copyWith({
    String? key,
    String? value,
    bool? secret,
  }) => DevServerEnvironmentVariable(
    key: key ?? this.key,
    value: value ?? this.value,
    secret: secret ?? this.secret,
  );
}

class DevServerConfig {
  const DevServerConfig({
    this.id = '',
    required this.projectId,
    required this.framework,
    required this.startupCommand,
    required this.port,
    required this.worktreePath,
    this.host = '127.0.0.1',
    this.environment = const [],
    this.autoStart = false,
    this.requiredForPreview = false,
    this.worktreeId,
  });

  final String id;
  final String projectId;
  final String framework;
  final String startupCommand;
  final String host;
  final int port;
  final String worktreePath;
  final List<DevServerEnvironmentVariable> environment;
  final bool autoStart;
  final bool requiredForPreview;
  final String? worktreeId;

  String get url => 'http://$host:$port';

  DevServerConfig copyWith({
    String? id,
    String? projectId,
    String? framework,
    String? startupCommand,
    String? host,
    int? port,
    String? worktreePath,
    List<DevServerEnvironmentVariable>? environment,
    bool? autoStart,
    bool? requiredForPreview,
    String? worktreeId,
  }) => DevServerConfig(
    id: id ?? this.id,
    projectId: projectId ?? this.projectId,
    framework: framework ?? this.framework,
    startupCommand: startupCommand ?? this.startupCommand,
    host: host ?? this.host,
    port: port ?? this.port,
    worktreePath: worktreePath ?? this.worktreePath,
    environment: environment ?? this.environment,
    autoStart: autoStart ?? this.autoStart,
    requiredForPreview: requiredForPreview ?? this.requiredForPreview,
    worktreeId: worktreeId ?? this.worktreeId,
  );
}

class DevServerHistoryEntry {
  const DevServerHistoryEntry({
    required this.kind,
    required this.actor,
    required this.createdAt,
  });
  final String kind;
  final String actor;
  final DateTime createdAt;
}

class DevServerPreviewMetadata {
  const DevServerPreviewMetadata({
    required this.url,
    required this.port,
    required this.status,
    this.metadata = const {},
  });
  final String url;
  final int port;
  final DevServerStatus status;
  final Map<String, dynamic> metadata;
}

class DevServerSnapshot {
  const DevServerSnapshot({
    required this.config,
    required this.status,
    this.instanceId = '',
    this.startedAt,
    this.stoppedAt,
    this.message,
    this.suggestedPort,
    this.history = const [],
    this.previewMetadata = const {},
  });

  final String instanceId;
  final DevServerConfig config;
  final DevServerStatus status;
  final DateTime? startedAt;
  final DateTime? stoppedAt;
  final String? message;
  final int? suggestedPort;
  final List<DevServerHistoryEntry> history;
  final Map<String, dynamic> previewMetadata;

  bool get running => status == DevServerStatus.running;
  bool get transitioning =>
      status == DevServerStatus.starting || status == DevServerStatus.stopping;

  DevServerSnapshot copyWith({
    String? instanceId,
    DevServerConfig? config,
    DevServerStatus? status,
    DateTime? startedAt,
    DateTime? stoppedAt,
    String? message,
    int? suggestedPort,
    List<DevServerHistoryEntry>? history,
    Map<String, dynamic>? previewMetadata,
  }) => DevServerSnapshot(
    instanceId: instanceId ?? this.instanceId,
    config: config ?? this.config,
    status: status ?? this.status,
    startedAt: startedAt ?? this.startedAt,
    stoppedAt: stoppedAt ?? this.stoppedAt,
    message: message ?? this.message,
    suggestedPort: suggestedPort ?? this.suggestedPort,
    history: history ?? this.history,
    previewMetadata: previewMetadata ?? this.previewMetadata,
  );
}

sealed class DevServerEvent {
  const DevServerEvent({required this.projectId});
  final String projectId;
}

class DevServerChanged extends DevServerEvent {
  const DevServerChanged({required super.projectId, required this.snapshot});
  final DevServerSnapshot snapshot;
}

class DevServerLog extends DevServerEvent {
  const DevServerLog({
    required super.projectId,
    required this.text,
    this.stderr = false,
  });
  final String text;
  final bool stderr;
}
