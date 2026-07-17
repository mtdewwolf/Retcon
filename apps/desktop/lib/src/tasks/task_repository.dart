import 'task_models.dart';

/// Desktop-facing seam for the Phase 21 task APIs. The core adapter can map its
/// wire format here without leaking protocol types into widgets or controllers.
abstract interface class TaskRepository {
  Future<List<RoadmapTask>> listTasks({String? projectId});
  Future<RoadmapTask> saveTask(RoadmapTask task, {String? projectId});
  Future<List<TaskSavedView>> listSavedViews({String? projectId});
  Future<TaskSavedView> saveView(TaskSavedView view, {String? projectId});
}

/// Deterministic repository used by widget tests and while the core adapter is
/// implemented in parallel.
class InMemoryTaskRepository implements TaskRepository {
  InMemoryTaskRepository({
    List<RoadmapTask> tasks = const [],
    List<TaskSavedView> views = const [],
  }) : _tasks = [...tasks],
       _views = [...views];

  final List<RoadmapTask> _tasks;
  final List<TaskSavedView> _views;

  @override
  Future<List<RoadmapTask>> listTasks({String? projectId}) async => [..._tasks];

  @override
  Future<RoadmapTask> saveTask(RoadmapTask task, {String? projectId}) async {
    final index = _tasks.indexWhere((item) => item.id == task.id);
    if (index < 0) {
      _tasks.add(task);
    } else {
      _tasks[index] = task;
    }
    return task;
  }

  @override
  Future<List<TaskSavedView>> listSavedViews({String? projectId}) async => [
    ..._views,
  ];

  @override
  Future<TaskSavedView> saveView(
    TaskSavedView view, {
    String? projectId,
  }) async {
    final index = _views.indexWhere((item) => item.id == view.id);
    if (index < 0) {
      _views.add(view);
    } else {
      _views[index] = view;
    }
    return view;
  }

  factory InMemoryTaskRepository.demo() => InMemoryTaskRepository(
    tasks: const [
      RoadmapTask(
        id: 'phase-21',
        title: 'Evidence-based task completion',
        phase: 21,
        status: TaskStatus.inProgress,
        assignee: 'Desktop agent',
        planApproved: true,
        tags: ['desktop', 'verification'],
        steps: [
          PlanStep(
            id: 'board',
            title: 'Build task board and saved views',
            status: PlanStepStatus.inProgress,
            assignee: 'Desktop agent',
          ),
          PlanStep(id: 'verify', title: 'Attach completion evidence'),
        ],
        criteria: [
          AcceptanceCriterion(
            id: 'gate',
            title: 'Required checks pass before completion',
          ),
        ],
      ),
      RoadmapTask(
        id: 'phase-22',
        title: 'Verification history',
        phase: 22,
        status: TaskStatus.planned,
      ),
    ],
  );
}
