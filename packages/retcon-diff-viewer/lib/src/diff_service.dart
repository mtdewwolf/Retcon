import 'diff_models.dart';

typedef GitRequest = Future<Map<String, dynamic>> Function(
  String method, {
  Map<String, dynamic> params,
});

/// RPC adapter for `git.*` methods used by the diff viewer.
abstract class DiffService {
  Future<String> diff({
    required String repo,
    DiffScope scope,
    String? path,
  });
  Future<void> stageHunk({required String repo, required String patch});
  Future<void> discardHunk({required String repo, required String patch});
  Future<void> stage({required String repo, String? path});
  Future<void> unstage({required String repo, String? path});
  Future<String> commit({required String repo, required String message});
}

class RpcDiffService implements DiffService {
  RpcDiffService(this._request);
  final GitRequest _request;

  @override
  Future<String> diff({
    required String repo,
    DiffScope scope = DiffScope.unstaged,
    String? path,
  }) async {
    final result = await _request(
      'git.diff',
      params: {
        'repo': repo,
        'mode': switch (scope) {
          DiffScope.unstaged => 'unstaged',
          DiffScope.staged => 'staged',
          DiffScope.all => 'all',
        },
        if (path != null) 'path': path,
      },
    );
    return result['diff'] as String? ?? '';
  }

  @override
  Future<void> stageHunk({required String repo, required String patch}) async {
    await _request('git.stageHunk', params: {'repo': repo, 'patch': patch});
  }

  @override
  Future<void> discardHunk({
    required String repo,
    required String patch,
  }) async {
    await _request('git.discardHunk', params: {'repo': repo, 'patch': patch});
  }

  @override
  Future<void> stage({required String repo, String? path}) async {
    await _request(
      'git.stage',
      params: {
        'repo': repo,
        if (path != null) 'path': path,
      },
    );
  }

  @override
  Future<void> unstage({required String repo, String? path}) async {
    await _request(
      'git.unstage',
      params: {
        'repo': repo,
        if (path != null) 'path': path,
      },
    );
  }

  @override
  Future<String> commit({required String repo, required String message}) async {
    final result = await _request(
      'git.commit',
      params: {'repo': repo, 'message': message},
    );
    return result['oid'] as String? ?? '';
  }
}

class FakeDiffService implements DiffService {
  FakeDiffService({this.diffText = ''});

  final String diffText;
  String? lastPatch;
  String? lastCommitMessage;

  @override
  Future<String> diff({
    required String repo,
    DiffScope scope = DiffScope.unstaged,
    String? path,
  }) async =>
      diffText;

  @override
  Future<void> stageHunk({required String repo, required String patch}) async {
    lastPatch = patch;
  }

  @override
  Future<void> discardHunk({
    required String repo,
    required String patch,
  }) async {
    lastPatch = patch;
  }

  @override
  Future<void> stage({required String repo, String? path}) async {}

  @override
  Future<void> unstage({required String repo, String? path}) async {}

  @override
  Future<String> commit({required String repo, required String message}) async {
    lastCommitMessage = message;
    return 'deadbeef';
  }
}
