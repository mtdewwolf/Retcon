/// Models for filesystem RPC payloads.
class FileEntry {
  const FileEntry({
    required this.name,
    required this.path,
    required this.isDirectory,
    required this.size,
    this.gitStatus,
  });

  final String name;
  final String path;
  final bool isDirectory;
  final int size;
  final String? gitStatus;

  factory FileEntry.fromJson(Map<String, dynamic> json) => FileEntry(
    name: json['name']?.toString() ?? '',
    path: json['path']?.toString() ?? '',
    isDirectory: json['isDirectory'] as bool? ?? false,
    size: (json['size'] as num?)?.toInt() ?? 0,
    gitStatus: json['gitStatus']?.toString(),
  );
}

class FileReadResult {
  const FileReadResult({
    required this.path,
    required this.size,
    required this.truncated,
    required this.binary,
    required this.language,
    this.content,
    this.contentBase64,
  });

  final String path;
  final String? content;
  final String? contentBase64;
  final int size;
  final bool truncated;
  final bool binary;
  final String language;

  factory FileReadResult.fromJson(Map<String, dynamic> json) => FileReadResult(
    path: json['path']?.toString() ?? '',
    content: json['content']?.toString(),
    contentBase64: json['contentBase64']?.toString(),
    size: (json['size'] as num?)?.toInt() ?? 0,
    truncated: json['truncated'] as bool? ?? false,
    binary: json['binary'] as bool? ?? false,
    language: json['language']?.toString() ?? 'plaintext',
  );
}

class FileWriteResult {
  const FileWriteResult({required this.path, required this.size});
  final String path;
  final int size;

  factory FileWriteResult.fromJson(Map<String, dynamic> json) => FileWriteResult(
    path: json['path']?.toString() ?? '',
    size: (json['size'] as num?)?.toInt() ?? 0,
  );
}

class FileWatchHandle {
  const FileWatchHandle({required this.watchId, required this.root});
  final String watchId;
  final String root;

  factory FileWatchHandle.fromJson(Map<String, dynamic> json) => FileWatchHandle(
    watchId: json['watchId']?.toString() ?? '',
    root: json['root']?.toString() ?? '',
  );
}

class FileChangeEvent {
  const FileChangeEvent({
    required this.watchId,
    required this.path,
    required this.change,
  });

  final String watchId;
  final String path;
  final String change;

  factory FileChangeEvent.fromPayload(Map<String, dynamic> payload) =>
      FileChangeEvent(
        watchId: payload['watchId']?.toString() ?? '',
        path: payload['path']?.toString() ?? '',
        change: payload['change']?.toString() ?? 'changed',
      );
}
