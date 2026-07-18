import 'file_models.dart';

typedef FileRequest =
    Future<Map<String, dynamic>> Function(
      String method, {
      Map<String, dynamic> params,
    });

/// RPC adapter for `file.*` methods on retcon-core.
abstract class FileService {
  Future<List<FileEntry>> list({required String root, String? path});
  Future<FileReadResult> read({
    required String root,
    required String path,
    int? limit,
  });
  Future<FileWriteResult> write({
    required String root,
    required String path,
    required String content,
    required String ifMatch,
    int? limit,
  });
  Future<FileWatchHandle> watch({
    required String root,
    String? path,
    String? watchId,
  });
  Future<bool> unwatch({required String watchId});
}

class RpcFileService implements FileService {
  RpcFileService(this._request);
  final FileRequest _request;

  @override
  Future<List<FileEntry>> list({required String root, String? path}) async {
    final result = await _request(
      'file.list',
      params: {'root': root, if (path != null) 'path': path},
    );
    return (result['entries'] as List? ?? const [])
        .map(
          (item) => FileEntry.fromJson(
            (item as Map?)?.cast<String, dynamic>() ?? const {},
          ),
        )
        .toList();
  }

  @override
  Future<FileReadResult> read({
    required String root,
    required String path,
    int? limit,
  }) async {
    final result = await _request(
      'file.read',
      params: {'root': root, 'path': path, if (limit != null) 'limit': limit},
    );
    return FileReadResult.fromJson(result);
  }

  @override
  Future<FileWriteResult> write({
    required String root,
    required String path,
    required String content,
    required String ifMatch,
    int? limit,
  }) async {
    final result = await _request(
      'file.write',
      params: {
        'root': root,
        'path': path,
        'content': content,
        'ifMatch': ifMatch,
        if (limit != null) 'limit': limit,
      },
    );
    return FileWriteResult.fromJson(result);
  }

  @override
  Future<FileWatchHandle> watch({
    required String root,
    String? path,
    String? watchId,
  }) async {
    final result = await _request(
      'file.watch',
      params: {
        'root': root,
        if (path != null) 'path': path,
        if (watchId != null) 'watchId': watchId,
      },
    );
    return FileWatchHandle.fromJson(result);
  }

  @override
  Future<bool> unwatch({required String watchId}) async {
    final result = await _request('file.unwatch', params: {'watchId': watchId});
    return result['stopped'] as bool? ?? false;
  }
}

class FakeFileService implements FileService {
  FakeFileService({this.entries = const [], this.readResult, this.writeResult});

  final List<FileEntry> entries;
  final FileReadResult? readResult;
  final FileWriteResult? writeResult;

  @override
  Future<List<FileEntry>> list({required String root, String? path}) async =>
      entries;

  @override
  Future<FileReadResult> read({
    required String root,
    required String path,
    int? limit,
  }) async =>
      readResult ??
      FileReadResult(
        path: path,
        content: '',
        size: 0,
        truncated: false,
        binary: false,
        language: 'plaintext',
      );

  @override
  Future<FileWriteResult> write({
    required String root,
    required String path,
    required String content,
    required String ifMatch,
    int? limit,
  }) async => writeResult ?? FileWriteResult(path: path, size: content.length);

  @override
  Future<FileWatchHandle> watch({
    required String root,
    String? path,
    String? watchId,
  }) async => FileWatchHandle(watchId: watchId ?? 'watch-1', root: root);

  @override
  Future<bool> unwatch({required String watchId}) async => true;
}
