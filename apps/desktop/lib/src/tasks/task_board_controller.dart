import 'package:flutter/foundation.dart';

import 'task_models.dart';
import 'task_repository.dart';

class TaskBoardController extends ChangeNotifier {
  TaskBoardController({required TaskRepository repository, this.projectId})
    : _repository = repository;

  final TaskRepository _repository;
  final String? projectId;

  List<RoadmapTask> _tasks = const [];
  List<TaskSavedView> _savedViews = const [];
  String _query = '';
  Set<TaskStatus> _statuses = {};
  TaskGrouping _grouping = TaskGrouping.status;
  String? _selectedTaskId;
  bool _loading = false;
  String? _error;

  List<RoadmapTask> get tasks => _tasks;
  List<TaskSavedView> get savedViews => _savedViews;
  String get query => _query;
  Set<TaskStatus> get statuses => _statuses;
  TaskGrouping get grouping => _grouping;
  bool get loading => _loading;
  String? get error => _error;
  RoadmapTask? get selectedTask {
    for (final task in _tasks) {
      if (task.id == _selectedTaskId) return task;
    }
    return null;
  }

  List<RoadmapTask> get visibleTasks {
    final normalized = _query.trim().toLowerCase();
    return _tasks.where((task) {
      final matchesStatus =
          _statuses.isEmpty || _statuses.contains(task.status);
      final haystack =
          '${task.title} ${task.assignee ?? ''} ${task.tags.join(' ')} '
                  '${task.phase ?? ''}'
              .toLowerCase();
      return matchesStatus &&
          (normalized.isEmpty || haystack.contains(normalized));
    }).toList();
  }

  Map<String, List<RoadmapTask>> get groupedTasks {
    final groups = <String, List<RoadmapTask>>{};
    for (final task in visibleTasks) {
      final key = switch (_grouping) {
        TaskGrouping.status => task.status.label,
        TaskGrouping.phase =>
          task.phase == null ? 'No phase' : 'Phase ${task.phase}',
        TaskGrouping.assignee => task.assignee ?? 'Unassigned',
        TaskGrouping.none => 'Tasks',
      };
      groups.putIfAbsent(key, () => []).add(task);
    }
    return groups;
  }

  Future<void> refresh() async {
    _loading = true;
    _error = null;
    notifyListeners();
    try {
      final values = await Future.wait([
        _repository.listTasks(projectId: projectId),
        _repository.listSavedViews(projectId: projectId),
      ]);
      _tasks = values[0] as List<RoadmapTask>;
      _savedViews = values[1] as List<TaskSavedView>;
      if (_selectedTaskId == null && _tasks.isNotEmpty) {
        _selectedTaskId = _tasks.first.id;
      }
    } on Object catch (error) {
      _error = error.toString();
    } finally {
      _loading = false;
      notifyListeners();
    }
  }

  void selectTask(String id) {
    _selectedTaskId = id;
    notifyListeners();
  }

  void setQuery(String value) {
    _query = value;
    notifyListeners();
  }

  void toggleStatus(TaskStatus status) {
    _statuses = {..._statuses};
    _statuses.contains(status)
        ? _statuses.remove(status)
        : _statuses.add(status);
    notifyListeners();
  }

  void setGrouping(TaskGrouping value) {
    _grouping = value;
    notifyListeners();
  }

  void applyView(TaskSavedView view) {
    _query = view.query;
    _statuses = {...view.statuses};
    _grouping = view.grouping;
    notifyListeners();
  }

  Future<void> saveCurrentView(String name) async {
    final trimmed = name.trim();
    if (trimmed.isEmpty) return;
    final view = TaskSavedView(
      id: 'view-${DateTime.now().microsecondsSinceEpoch}',
      name: trimmed,
      query: _query,
      statuses: {..._statuses},
      grouping: _grouping,
    );
    await _repository.saveView(view, projectId: projectId);
    _savedViews = [..._savedViews, view];
    notifyListeners();
  }

  Future<void> setTaskStatus(TaskStatus status) async {
    final task = selectedTask;
    if (task == null || (status == TaskStatus.complete && !task.canComplete)) {
      return;
    }
    await _save(task.copyWith(status: status));
  }

  Future<void> approvePlan(bool approved) async {
    final task = selectedTask;
    if (task != null) await _save(task.copyWith(planApproved: approved));
  }

  Future<void> addStep(String title) async {
    final task = selectedTask;
    final trimmed = title.trim();
    if (task == null || trimmed.isEmpty) return;
    await _save(
      task.copyWith(
        steps: [
          ...task.steps,
          PlanStep(
            id: 'step-${DateTime.now().microsecondsSinceEpoch}',
            title: trimmed,
          ),
        ],
      ),
    );
  }

  Future<void> deleteStep(String id) async {
    final task = selectedTask;
    if (task != null) {
      await _save(
        task.copyWith(
          steps: task.steps.where((step) => step.id != id).toList(),
        ),
      );
    }
  }

  Future<void> moveStep(String id, int delta) async {
    final task = selectedTask;
    if (task == null) return;
    final oldIndex = task.steps.indexWhere((step) => step.id == id);
    final newIndex = oldIndex + delta;
    if (oldIndex < 0 || newIndex < 0 || newIndex >= task.steps.length) return;
    final steps = [...task.steps];
    final step = steps.removeAt(oldIndex);
    steps.insert(newIndex, step);
    await _save(task.copyWith(steps: steps));
  }

  Future<void> updateStep(
    String id, {
    String? title,
    PlanStepStatus? status,
    String? assignee,
    String? evidence,
  }) async {
    final task = selectedTask;
    if (task == null) return;
    final steps = task.steps.map((step) {
      if (step.id != id) return step;
      return step.copyWith(
        title: title,
        status: status,
        assignee: assignee,
        evidence: evidence == null || evidence.trim().isEmpty
            ? step.evidence
            : [...step.evidence, evidence.trim()],
      );
    }).toList();
    await _save(task.copyWith(steps: steps));
  }

  Future<void> setCriterionStatus(String id, CriterionStatus status) async {
    final task = selectedTask;
    if (task == null) return;
    await _save(
      task.copyWith(
        criteria: task.criteria
            .map(
              (criterion) => criterion.id == id
                  ? criterion.copyWith(status: status)
                  : criterion,
            )
            .toList(),
      ),
    );
  }

  Future<void> _save(RoadmapTask task) async {
    final saved = await _repository.saveTask(task, projectId: projectId);
    _tasks = _tasks.map((item) => item.id == saved.id ? saved : item).toList();
    notifyListeners();
  }
}
