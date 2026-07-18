import 'dart:async';

import 'package:flutter/foundation.dart';

import '../core_client.dart';
import 'project_models.dart';
import 'project_service.dart';

typedef ProjectOpenedCallback = void Function(OpenProjectResult project);

/// Tracks recent projects and the active project for shell chrome.
class ProjectController extends ChangeNotifier {
  ProjectController({
    required ProjectService service,
    CoreClient? core,
    this.onProjectOpened,
  }) : _service = service {
    if (core != null) {
      _events = core.events.listen(_handleEvent);
    }
  }

  factory ProjectController.fromCore(
    CoreClient core, {
    ProjectOpenedCallback? onProjectOpened,
  }) => ProjectController(
    service: CoreProjectService(core),
    core: core,
    onProjectOpened: onProjectOpened,
  );

  final ProjectService _service;
  final ProjectOpenedCallback? onProjectOpened;
  StreamSubscription<Map<String, dynamic>>? _events;

  List<ProjectSummary> _projects = const [];
  OpenProjectResult? _current;
  bool _loading = false;
  Object? _error;

  List<ProjectSummary> get projects => _projects;
  OpenProjectResult? get current => _current;
  bool get loading => _loading;
  Object? get error => _error;

  String get projectTitle => _current?.metadata.name ?? 'No project open';
  String get branch => _current?.branch ?? '—';

  List<ProjectSummary> get pinnedProjects =>
      _projects.where((project) => project.pinned).toList();

  List<ProjectSummary> get recentProjects =>
      _projects.where((project) => !project.pinned).toList();

  Future<void> refresh({String? query}) async {
    _loading = true;
    _error = null;
    notifyListeners();
    try {
      final records = await _service.list(query: query);
      records.sort((a, b) {
        if (a.pinned != b.pinned) return a.pinned ? -1 : 1;
        return (b.updatedAt ?? '').compareTo(a.updatedAt ?? '');
      });
      _projects = records;
    } catch (error) {
      _error = error;
    } finally {
      _loading = false;
      notifyListeners();
    }
  }

  Future<OpenProjectResult> open(String path) async {
    _loading = true;
    _error = null;
    notifyListeners();
    try {
      final opened = await _service.open(path);
      _applyOpened(opened);
      await refresh();
      return opened;
    } catch (error) {
      _error = error;
      rethrow;
    } finally {
      _loading = false;
      notifyListeners();
    }
  }

  Future<OpenProjectResult> clone({
    required String remoteUrl,
    required String destination,
  }) async {
    _loading = true;
    _error = null;
    notifyListeners();
    try {
      final opened = await _service.clone(
        remoteUrl: remoteUrl,
        destination: destination,
      );
      _applyOpened(opened);
      await refresh();
      return opened;
    } catch (error) {
      _error = error;
      rethrow;
    } finally {
      _loading = false;
      notifyListeners();
    }
  }

  Future<void> togglePinned(ProjectSummary project) async {
    await _service.updateMetadata(project.id, {'pinned': !project.pinned});
    await refresh();
  }

  Future<void> remove(ProjectSummary project) async {
    await _service.remove(project.id);
    if (_current?.id == project.id) {
      _current = null;
    }
    await refresh();
  }

  void _applyOpened(OpenProjectResult opened) {
    _current = opened;
    onProjectOpened?.call(opened);
    notifyListeners();
  }

  void _handleEvent(Map<String, dynamic> event) {
    final type = event['type']?.toString();
    switch (type) {
      case 'project.opened':
      case 'project.metadataUpdated':
        unawaited(refresh());
      case 'project.removed':
        final projectId = event['projectId']?.toString();
        if (projectId != null && _current?.id == projectId) {
          _current = null;
        }
        unawaited(refresh());
      default:
        break;
    }
  }

  @override
  void dispose() {
    unawaited(_events?.cancel());
    super.dispose();
  }
}
