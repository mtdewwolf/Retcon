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
    required this.projectId,
    required this.framework,
    required this.startupCommand,
    required this.port,
    required this.worktreePath,
    this.host = '127.0.0.1',
    this.environment = const [],
    this.autoStart = false,
    this.requiredForPreview = false,
  });

  final String projectId;
  final String framework;
  final String startupCommand;
  final String host;
  final int port;
  final String worktreePath;
  final List<DevServerEnvironmentVariable> environment;
  final bool autoStart;
  final bool requiredForPreview;

  String get url => 'http://$host:$port';

  DevServerConfig copyWith({
    String? projectId,
    String? framework,
    String? startupCommand,
    String? host,
    int? port,
    String? worktreePath,
    List<DevServerEnvironmentVariable>? environment,
    bool? autoStart,
    bool? requiredForPreview,
  }) => DevServerConfig(
    projectId: projectId ?? this.projectId,
    framework: framework ?? this.framework,
    startupCommand: startupCommand ?? this.startupCommand,
    host: host ?? this.host,
    port: port ?? this.port,
    worktreePath: worktreePath ?? this.worktreePath,
    environment: environment ?? this.environment,
    autoStart: autoStart ?? this.autoStart,
    requiredForPreview: requiredForPreview ?? this.requiredForPreview,
  );
}

class DevServerSnapshot {
  const DevServerSnapshot({
    required this.config,
    required this.status,
    this.startedAt,
    this.stoppedAt,
    this.message,
    this.suggestedPort,
  });

  final DevServerConfig config;
  final DevServerStatus status;
  final DateTime? startedAt;
  final DateTime? stoppedAt;
  final String? message;
  final int? suggestedPort;

  bool get running => status == DevServerStatus.running;
  bool get transitioning =>
      status == DevServerStatus.starting || status == DevServerStatus.stopping;

  DevServerSnapshot copyWith({
    DevServerConfig? config,
    DevServerStatus? status,
    DateTime? startedAt,
    DateTime? stoppedAt,
    String? message,
    int? suggestedPort,
  }) => DevServerSnapshot(
    config: config ?? this.config,
    status: status ?? this.status,
    startedAt: startedAt ?? this.startedAt,
    stoppedAt: stoppedAt ?? this.stoppedAt,
    message: message ?? this.message,
    suggestedPort: suggestedPort ?? this.suggestedPort,
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
