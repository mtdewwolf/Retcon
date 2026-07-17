import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_desktop/src/tasks/tasks.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

void main() {
  group('RoadmapTask completion gate', () {
    const step = PlanStep(
      id: 'step',
      title: 'Implement',
      status: PlanStepStatus.complete,
    );
    const criterion = AcceptanceCriterion(
      id: 'criterion',
      title: 'Tests pass',
      status: CriterionStatus.passed,
    );

    test('requires plan approval, completed steps, and passed criteria', () {
      const task = RoadmapTask(
        id: 'task',
        title: 'Feature',
        status: TaskStatus.review,
        planApproved: true,
        steps: [step],
        criteria: [criterion],
      );

      expect(task.canComplete, isTrue);
      expect(task.copyWith(planApproved: false).canComplete, isFalse);
      expect(
        task
            .copyWith(steps: [step.copyWith(status: PlanStepStatus.inProgress)])
            .canComplete,
        isFalse,
      );
      expect(
        task
            .copyWith(
              criteria: [criterion.copyWith(status: CriterionStatus.failed)],
            )
            .canComplete,
        isFalse,
      );
    });
  });

  group('TaskBoardController', () {
    late InMemoryTaskRepository repository;
    late TaskBoardController controller;

    setUp(() {
      repository = InMemoryTaskRepository(tasks: _tasks);
      controller = TaskBoardController(repository: repository);
    });

    tearDown(() => controller.dispose());

    test('creates a backlog task and selects it', () async {
      await controller.refresh();
      await controller.createTask('Ship checkpoints UI');

      expect(controller.tasks.first.title, 'Ship checkpoints UI');
      expect(controller.tasks.first.status, TaskStatus.backlog);
      expect(controller.selectedTask?.title, 'Ship checkpoints UI');
    });

    test('filters, groups, and restores a saved view', () async {
      await controller.refresh();
      controller.setQuery('desktop');
      controller.toggleStatus(TaskStatus.inProgress);
      controller.setGrouping(TaskGrouping.assignee);

      expect(controller.visibleTasks.map((task) => task.id), ['one']);
      expect(controller.groupedTasks.keys, ['Agent A']);

      await controller.saveCurrentView('My work');
      controller.setQuery('');
      controller.toggleStatus(TaskStatus.inProgress);
      controller.setGrouping(TaskGrouping.status);
      controller.applyView(controller.savedViews.single);

      expect(controller.query, 'desktop');
      expect(controller.statuses, {TaskStatus.inProgress});
      expect(controller.grouping, TaskGrouping.assignee);
    });

    test('supports plan editing and refuses premature completion', () async {
      await controller.refresh();
      controller.selectTask('one');

      await controller.setTaskStatus(TaskStatus.complete);
      expect(controller.selectedTask!.status, TaskStatus.inProgress);

      await controller.addStep('Document evidence');
      final added = controller.selectedTask!.steps.last;
      await controller.updateStep(
        added.id,
        title: 'Document verification evidence',
        status: PlanStepStatus.complete,
        assignee: 'Agent B',
        evidence: 'flutter test',
      );
      await controller.moveStep(added.id, -1);

      expect(
        controller.selectedTask!.steps.first.title,
        'Document verification evidence',
      );
      expect(controller.selectedTask!.steps.first.assignee, 'Agent B');
      expect(controller.selectedTask!.steps.first.evidence, ['flutter test']);
    });
  });

  testWidgets('board supports search and adding plan steps', (tester) async {
    await _setSize(tester);
    final repository = InMemoryTaskRepository(tasks: _tasks);
    await tester.pumpWidget(_app(TaskBoardPanel(repository: repository)));
    await tester.pumpAndSettle();

    expect(find.text('Desktop task'), findsAtLeastNWidgets(1));
    expect(find.text('Acceptance criteria'), findsOneWidget);

    await tester.enterText(find.byKey(const Key('task-search')), 'no match');
    await tester.pump();
    expect(find.text('No tasks match this view.'), findsOneWidget);

    await tester.enterText(find.byKey(const Key('task-search')), 'desktop');
    await tester.pump();
    await tester.tap(find.byKey(const Key('add-step')));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextFormField), 'Capture screenshot');
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();

    expect(find.text('Capture screenshot'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('completion remains disabled until acceptance criteria pass', (
    tester,
  ) async {
    await _setSize(tester);
    final repository = InMemoryTaskRepository(
      tasks: const [
        RoadmapTask(
          id: 'ready',
          title: 'Ready after verification',
          status: TaskStatus.review,
          planApproved: true,
          steps: [
            PlanStep(
              id: 'done',
              title: 'Implementation',
              status: PlanStepStatus.complete,
            ),
          ],
          criteria: [AcceptanceCriterion(id: 'tests', title: 'Tests pass')],
        ),
      ],
    );
    await tester.pumpWidget(_app(TaskBoardPanel(repository: repository)));
    await tester.pumpAndSettle();

    expect(find.byKey(const Key('completion-gate-message')), findsOneWidget);
    expect(
      tester
          .widget<FilledButton>(find.byKey(const Key('complete-task')))
          .onPressed,
      isNull,
    );

    await tester.tap(find.byKey(const Key('criterion-tests')));
    await tester.pumpAndSettle();
    expect(find.byKey(const Key('completion-gate-message')), findsNothing);
    expect(
      tester
          .widget<FilledButton>(find.byKey(const Key('complete-task')))
          .onPressed,
      isNotNull,
    );

    await tester.ensureVisible(find.byKey(const Key('complete-task')));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const Key('complete-task')));
    await tester.pumpAndSettle();
    final stored = await repository.listTasks();
    expect(stored.single.status, TaskStatus.complete);
  });
}

const _tasks = [
  RoadmapTask(
    id: 'one',
    title: 'Desktop task',
    phase: 21,
    status: TaskStatus.inProgress,
    assignee: 'Agent A',
    planApproved: true,
    tags: ['desktop'],
    steps: [PlanStep(id: 'first', title: 'Build board')],
    criteria: [AcceptanceCriterion(id: 'verified', title: 'Widget tests pass')],
  ),
  RoadmapTask(
    id: 'two',
    title: 'Core task',
    phase: 21,
    status: TaskStatus.planned,
    assignee: 'Agent B',
  ),
];

Widget _app(Widget child) => MaterialApp(
  theme: buildLunaDarkTheme(),
  home: Scaffold(body: child),
);

Future<void> _setSize(WidgetTester tester) async {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = const Size(1200, 820);
  addTearDown(tester.view.resetDevicePixelRatio);
  addTearDown(tester.view.resetPhysicalSize);
}
