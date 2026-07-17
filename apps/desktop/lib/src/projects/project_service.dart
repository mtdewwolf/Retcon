import '../core_client.dart';
import 'project_models.dart';

/// RPC adapter for `project.*` methods on retcon-core.
abstract class ProjectService {
  Future<List<ProjectSummary>> list({String? query});
  Future<OpenProjectResult> open(String path);
  Future<OpenProjectResult> clone({
    required String remoteUrl,
    required String destination,
  });
  Future<ProjectHealth> inspect(String path);
  Future<ProjectMetadata> updateMetadata(
    String projectId,
    Map<String, dynamic> patch,
  );
  Future<void> remove(String projectId);
}

class CoreProjectService implements ProjectService {
  CoreProjectService(this._core);

  final CoreClient _core;

  @override
  Future<List<ProjectSummary>> list({String? query}) async {
    final result = await _core.request(
      'project.list',
      params: query == null ? const {} : {'query': query},
    );
    return (result['projects'] as List? ?? const [])
        .map(
          (item) => ProjectSummary.fromJson(
            (item as Map?)?.cast<String, dynamic>() ?? const {},
          ),
        )
        .toList();
  }

  @override
  Future<OpenProjectResult> open(String path) async {
    final result = await _core.request('project.open', params: {'path': path});
    return OpenProjectResult.fromJson(result);
  }

  @override
  Future<OpenProjectResult> clone({
    required String remoteUrl,
    required String destination,
  }) async {
    final result = await _core.request(
      'project.clone',
      params: {'remoteUrl': remoteUrl, 'destination': destination},
      timeout: const Duration(minutes: 5),
    );
    return OpenProjectResult.fromJson(result);
  }

  @override
  Future<ProjectHealth> inspect(String path) async {
    final result = await _core.request('project.inspect', params: {'path': path});
    return ProjectHealth.fromJson(
      (result['health'] as Map?)?.cast<String, dynamic>() ?? const {},
    );
  }

  @override
  Future<ProjectMetadata> updateMetadata(
    String projectId,
    Map<String, dynamic> patch,
  ) async {
    final result = await _core.request(
      'project.updateMetadata',
      params: {'projectId': projectId, 'metadata': patch},
    );
    return ProjectMetadata.fromJson(result);
  }

  @override
  Future<void> remove(String projectId) async {
    await _core.request(
      'project.remove',
      params: {'projectId': projectId},
    );
  }
}
