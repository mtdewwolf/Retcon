/// Typed views of project RPC payloads.
class ProjectSummary {
  const ProjectSummary({
    required this.id,
    required this.metadata,
    this.updatedAt,
  });

  factory ProjectSummary.fromJson(Map<String, dynamic> json) => ProjectSummary(
    id: json['id']?.toString() ?? '',
    metadata: ProjectMetadata.fromJson(
      (json['metadata'] as Map?)?.cast<String, dynamic>() ?? const {},
    ),
    updatedAt: json['updatedAt']?.toString(),
  );

  final String id;
  final ProjectMetadata metadata;
  final String? updatedAt;

  bool get pinned => metadata.pinned;
  String get name => metadata.name;
  String get repositoryPath => metadata.repositoryPath;
}

class ProjectMetadata {
  const ProjectMetadata({
    required this.name,
    required this.repositoryPath,
    this.remoteUrl,
    this.pinned = false,
    this.preferredProvider,
    this.testCommand,
  });

  factory ProjectMetadata.fromJson(Map<String, dynamic> json) => ProjectMetadata(
    name: json['name']?.toString() ?? 'Project',
    repositoryPath: json['repositoryPath']?.toString() ?? '',
    remoteUrl: json['remoteUrl']?.toString(),
    pinned: json['pinned'] == true,
    preferredProvider: json['preferredProvider']?.toString(),
    testCommand: json['testCommand']?.toString(),
  );

  final String name;
  final String repositoryPath;
  final String? remoteUrl;
  final bool pinned;
  final String? preferredProvider;
  final String? testCommand;

  Map<String, dynamic> toJson() => {
    'name': name,
    'repositoryPath': repositoryPath,
    if (remoteUrl != null) 'remoteUrl': remoteUrl,
    'pinned': pinned,
    if (preferredProvider != null) 'preferredProvider': preferredProvider,
    if (testCommand != null) 'testCommand': testCommand,
  };
}

class OpenProjectResult {
  const OpenProjectResult({
    required this.id,
    required this.metadata,
    required this.analysis,
    required this.health,
  });

  factory OpenProjectResult.fromJson(Map<String, dynamic> json) =>
      OpenProjectResult(
        id: json['id']?.toString() ?? '',
        metadata: ProjectMetadata.fromJson(
          (json['metadata'] as Map?)?.cast<String, dynamic>() ?? const {},
        ),
        analysis: (json['analysis'] as Map?)?.cast<String, dynamic>() ?? const {},
        health: ProjectHealth.fromJson(
          (json['health'] as Map?)?.cast<String, dynamic>() ?? const {},
        ),
      );

  final String id;
  final ProjectMetadata metadata;
  final Map<String, dynamic> analysis;
  final ProjectHealth health;

  String? get branch {
    for (final check in health.checks) {
      if (check.name == 'Branch detected') {
        return check.detail;
      }
    }
    return null;
  }
}

class ProjectHealth {
  const ProjectHealth({required this.checks});

  factory ProjectHealth.fromJson(Map<String, dynamic> json) => ProjectHealth(
    checks: (json['checks'] as List? ?? const [])
        .map(
          (item) => HealthCheck.fromJson(
            (item as Map?)?.cast<String, dynamic>() ?? const {},
          ),
        )
        .toList(),
  );

  final List<HealthCheck> checks;
}

class HealthCheck {
  const HealthCheck({
    required this.name,
    required this.status,
    this.detail,
  });

  factory HealthCheck.fromJson(Map<String, dynamic> json) => HealthCheck(
    name: json['name']?.toString() ?? 'Check',
    status: json['status']?.toString() ?? 'warning',
    detail: json['detail']?.toString(),
  );

  final String name;
  final String status;
  final String? detail;
}
