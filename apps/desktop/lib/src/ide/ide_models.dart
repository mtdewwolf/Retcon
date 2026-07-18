class IdeDescriptor {
  const IdeDescriptor({
    required this.id,
    required this.name,
    required this.available,
    this.version,
    this.capabilities = const [],
  });

  factory IdeDescriptor.fromJson(Map<String, dynamic> json) => IdeDescriptor(
    id: json['id']?.toString() ?? '',
    name: json['name']?.toString() ?? json['id']?.toString() ?? 'Editor',
    available: json['available'] == true || json['detected'] == true,
    version: json['version']?.toString(),
    capabilities: (json['capabilities'] as List? ?? const [])
        .map((value) => value.toString())
        .toList(growable: false),
  );

  final String id;
  final String name;
  final bool available;
  final String? version;
  final List<String> capabilities;
}

class IdeConfiguration {
  const IdeConfiguration({this.preferredIdeId});

  factory IdeConfiguration.fromJson(Map<String, dynamic> json) =>
      IdeConfiguration(preferredIdeId: json['preferredIdeId']?.toString());

  final String? preferredIdeId;
}

class IdeLaunchResult {
  const IdeLaunchResult({
    required this.launched,
    required this.ideId,
    required this.action,
  });

  factory IdeLaunchResult.fromJson(Map<String, dynamic> json) =>
      IdeLaunchResult(
        launched: json['launched'] == true,
        ideId: json['ideId']?.toString() ?? '',
        action: json['action']?.toString() ?? '',
      );

  final bool launched;
  final String ideId;
  final String action;
}

class SyncedFile {
  const SyncedFile({
    required this.path,
    required this.revision,
    required this.size,
    required this.truncated,
    required this.binary,
    required this.language,
    this.content,
  });

  factory SyncedFile.fromJson(Map<String, dynamic> json) => SyncedFile(
    path: json['path']?.toString() ?? '',
    revision: json['revision']?.toString() ?? '',
    content: json['content']?.toString(),
    size: (json['size'] as num?)?.toInt() ?? 0,
    truncated: json['truncated'] == true,
    binary: json['binary'] == true,
    language: json['language']?.toString() ?? 'plaintext',
  );

  final String path;
  final String revision;
  final String? content;
  final int size;
  final bool truncated;
  final bool binary;
  final String language;
}

class SyncedWriteResult {
  const SyncedWriteResult({
    required this.path,
    required this.revision,
    required this.size,
  });

  factory SyncedWriteResult.fromJson(Map<String, dynamic> json) =>
      SyncedWriteResult(
        path: json['path']?.toString() ?? '',
        revision: json['revision']?.toString() ?? '',
        size: (json['size'] as num?)?.toInt() ?? 0,
      );

  final String path;
  final String revision;
  final int size;
}

class FileRevisionConflict implements Exception {
  const FileRevisionConflict([
    this.message = 'This file changed outside Retcon.',
  ]);
  final String message;
  @override
  String toString() => message;
}

class SyncedFileMissing implements Exception {
  const SyncedFileMissing();

  @override
  String toString() => 'This file was removed outside Retcon.';
}
