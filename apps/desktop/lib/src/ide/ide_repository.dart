import '../core_client.dart';
import 'ide_models.dart';

abstract class IdeRepository {
  Future<List<IdeDescriptor>> detect();
  Future<IdeConfiguration> configuration();
  Future<IdeConfiguration> updatePreferred(String? ideId);
  Future<IdeLaunchResult> openProject(String path, {String? ideId});
  Future<IdeLaunchResult> openWorktree(String path, {String? ideId});
  Future<IdeLaunchResult> openFile({
    required String workspacePath,
    required String path,
    String? ideId,
    int? line,
    int? column,
  });
  Future<IdeLaunchResult> openDiff({
    required String workspacePath,
    required String leftPath,
    required String rightPath,
    String? ideId,
  });
  Future<IdeLaunchResult> openTerminalLocation({
    required String workspacePath,
    String? path,
    String? ideId,
  });
}

abstract class SyncedFileRepository {
  Future<SyncedFile> read({
    required String root,
    required String path,
    String? ifNoneMatch,
    int? limit,
  });
  Future<SyncedWriteResult> write({
    required String root,
    required String path,
    required String content,
    required String ifMatch,
  });
}

class CoreIdeRepository implements IdeRepository, SyncedFileRepository {
  CoreIdeRepository(this._core);
  final CoreClient _core;

  @override
  Future<List<IdeDescriptor>> detect() async {
    final result = await _core.request('ide.detect');
    return (result['ides'] as List? ?? result['editors'] as List? ?? const [])
        .map(
          (item) => IdeDescriptor.fromJson(
            (item as Map?)?.cast<String, dynamic>() ?? const {},
          ),
        )
        .where((item) => {'vscode', 'cursor', 'windsurf'}.contains(item.id))
        .toList();
  }

  @override
  Future<IdeConfiguration> configuration() async =>
      IdeConfiguration.fromJson(await _core.request('ide.configuration.get'));

  @override
  Future<IdeConfiguration> updatePreferred(String? ideId) async =>
      IdeConfiguration.fromJson(
        await _core.request(
          'ide.configuration.update',
          params: {'preferredIdeId': ideId},
        ),
      );

  Future<IdeLaunchResult> _launch(
    String method,
    Map<String, dynamic> params,
  ) async =>
      IdeLaunchResult.fromJson(await _core.request(method, params: params));

  @override
  Future<IdeLaunchResult> openProject(String path, {String? ideId}) =>
      _launch('ide.openProject', {'path': path, 'ideId': ?ideId});

  @override
  Future<IdeLaunchResult> openWorktree(String path, {String? ideId}) =>
      _launch('ide.openWorktree', {'path': path, 'ideId': ?ideId});

  @override
  Future<IdeLaunchResult> openFile({
    required String workspacePath,
    required String path,
    String? ideId,
    int? line,
    int? column,
  }) => _launch('ide.openFile', {
    'workspacePath': workspacePath,
    'path': path,
    'ideId': ?ideId,
    'line': ?line,
    'column': ?column,
  });

  @override
  Future<IdeLaunchResult> openDiff({
    required String workspacePath,
    required String leftPath,
    required String rightPath,
    String? ideId,
  }) => _launch('ide.openDiff', {
    'workspacePath': workspacePath,
    'leftPath': leftPath,
    'rightPath': rightPath,
    'ideId': ?ideId,
  });

  @override
  Future<IdeLaunchResult> openTerminalLocation({
    required String workspacePath,
    String? path,
    String? ideId,
  }) => _launch('ide.openTerminalLocation', {
    'workspacePath': workspacePath,
    'path': ?path,
    'ideId': ?ideId,
  });

  @override
  Future<SyncedFile> read({
    required String root,
    required String path,
    String? ifNoneMatch,
    int? limit,
  }) async => SyncedFile.fromJson(
    await _core.request(
      'file.read',
      params: {
        'root': root,
        'path': path,
        'ifNoneMatch': ?ifNoneMatch,
        'limit': ?limit,
      },
    ),
  );

  @override
  Future<SyncedWriteResult> write({
    required String root,
    required String path,
    required String content,
    required String ifMatch,
  }) async {
    try {
      return SyncedWriteResult.fromJson(
        await _core.request(
          'file.write',
          params: {
            'root': root,
            'path': path,
            'content': content,
            'ifMatch': ifMatch,
          },
        ),
      );
    } on CoreRpcException catch (error) {
      final details = error.details;
      final code = details is Map ? details['code']?.toString() : null;
      if (code == 'revision_conflict' ||
          code == 'conflict' ||
          error.message.toLowerCase().contains('changed')) {
        throw const FileRevisionConflict();
      }
      rethrow;
    }
  }
}
