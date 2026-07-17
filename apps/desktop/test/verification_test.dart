import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_desktop/src/tasks/tasks.dart';
import 'package:retcon_desktop/src/verification/verification.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

void main() {
  group('VerificationController', () {
    test(
      'edits commands and persists ordered required/optional gates',
      () async {
        final repository = InMemoryVerificationRepository(
          commands: _commands,
          gates: const {
            taskId: [analyzeGate],
          },
        );
        final controller = VerificationController(
          repository: repository,
          taskId: taskId,
        );
        addTearDown(controller.dispose);
        await controller.load();

        await controller.overrideCommand(
          controller.commands.first,
          'flutter analyze --fatal-infos',
        );
        await controller.addGate(controller.commands.last);
        await controller.updateGate(
          controller.gates.last.copyWith(required: false),
        );
        await controller.moveGate('test', -1);

        expect(controller.commands.first.source, CommandSource.override);
        expect(
          controller.commands.first.command,
          'flutter analyze --fatal-infos',
        );
        expect(controller.gates.map((gate) => gate.id), ['test', 'analyze']);
        expect(controller.gates.first.required, isFalse);
        expect(await repository.loadGates(taskId), controller.gates);
      },
    );

    test(
      'streams parsed results, bounds output, and unlocks completion',
      () async {
        final repository = InMemoryVerificationRepository(
          gates: const {
            taskId: [analyzeGate, optionalTestGate],
          },
          results: const {
            'analyze': ScriptedGateResult(
              tests: TestCounts(passed: 12, failed: 0, skipped: 1),
              duration: Duration(milliseconds: 640),
              stdout: '01234567890123456789',
              fileLinks: [VerificationFileLink(path: 'lib/app.dart', line: 42)],
            ),
            'test': ScriptedGateResult(
              passed: false,
              tests: TestCounts(failed: 1),
              stderr: 'optional snapshot mismatch',
            ),
          },
        );
        final controller = VerificationController(
          repository: repository,
          taskId: taskId,
          maxLogCharacters: 12,
        );
        addTearDown(controller.dispose);
        await controller.load();

        expect(controller.allowsCompletion, isFalse);
        await controller.runAll();
        await _flushEvents();

        expect(controller.latestRun?.status, VerificationRunStatus.failed);
        expect(controller.latestRun?.passedTests, 12);
        expect(
          controller.latestRun?.gates.first.duration,
          const Duration(milliseconds: 640),
        );
        expect(controller.latestRun?.gates.first.fileLinks.single.line, 42);
        expect(controller.latestRun?.gates.first.stdout, startsWith('…'));
        expect(controller.latestRun?.gates.first.stdout.length, 13);
        // Optional failures remain visible but do not block completion.
        expect(controller.allowsCompletion, isTrue);
      },
    );

    test('cancels active runs and supports history comparison', () async {
      final previous = _run(
        id: 'previous',
        status: VerificationRunStatus.passed,
        passed: 8,
        failed: 0,
        duration: const Duration(seconds: 2),
      );
      final current = _run(
        id: 'current',
        status: VerificationRunStatus.failed,
        passed: 7,
        failed: 1,
        duration: const Duration(seconds: 3),
      );
      final repository = InMemoryVerificationRepository(
        autoComplete: false,
        gates: const {
          taskId: [analyzeGate, optionalTestGate],
        },
        history: {
          taskId: [current, previous],
        },
      );
      final controller = VerificationController(
        repository: repository,
        taskId: taskId,
      );
      addTearDown(controller.dispose);
      await controller.load();

      expect(controller.comparison?.passedTestDelta, -1);
      expect(controller.comparison?.failedTestDelta, 1);
      expect(controller.comparison?.durationDelta, const Duration(seconds: 1));

      await controller.rerunFailed();
      expect(controller.running, isTrue);
      expect(controller.activeRun?.gates.map((gate) => gate.gateId), [
        'analyze',
      ]);
      await controller.cancel();
      await _flushEvents();
      expect(controller.latestRun?.status, VerificationRunStatus.cancelled);
    });
  });

  testWidgets(
    'verification center renders setup, live output, history, and report',
    (tester) async {
      await _setSize(tester, const Size(1100, 760));
      final repository = InMemoryVerificationRepository(
        commands: _commands,
        gates: const {
          taskId: [analyzeGate, optionalTestGate],
        },
        history: {
          taskId: [
            _run(
              id: 'latest',
              status: VerificationRunStatus.failed,
              passed: 4,
              failed: 1,
              duration: const Duration(milliseconds: 1500),
              stdout: 'test output',
            ),
            _run(
              id: 'previous',
              status: VerificationRunStatus.passed,
              passed: 4,
              failed: 0,
              duration: const Duration(seconds: 1),
            ),
          ],
        },
      );
      final controller = VerificationController(
        repository: repository,
        taskId: taskId,
      );
      addTearDown(controller.dispose);
      await controller.load();
      await tester.pumpWidget(
        _app(VerificationPanel(controller: controller, task: readyTask)),
      );

      expect(find.text('Project commands'), findsOneWidget);
      expect(find.text('Ordered verification gates'), findsOneWidget);
      expect(find.text('Required'), findsAtLeastNWidgets(1));
      expect(find.text('Optional'), findsAtLeastNWidgets(1));

      await tester.tap(find.text('Live run'));
      await tester.pumpAndSettle();
      expect(find.text('Gate timeline'), findsOneWidget);
      expect(find.textContaining('4 passed, 1 failed'), findsOneWidget);
      expect(find.byKey(const Key('verification-raw-output')), findsOneWidget);
      expect(find.text('test output'), findsOneWidget);

      await tester.tap(find.text('History'));
      await tester.pumpAndSettle();
      expect(find.text('Latest versus previous'), findsOneWidget);
      expect(find.text('+1'), findsOneWidget);

      await tester.tap(find.text('Completion report'));
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('completion-report-panel')), findsOneWidget);
      expect(find.text('Roadmap completion report'), findsOneWidget);
      expect(find.text('Phase'), findsOneWidget);
      expect(find.text('22'), findsOneWidget);
      expect(find.text('Required verification'), findsOneWidget);
    },
  );

  testWidgets(
    'failed required gates visibly block task completion until rerun passes',
    (tester) async {
      await _setSize(tester, const Size(1200, 860));
      final failed = _run(
        id: 'failed',
        status: VerificationRunStatus.failed,
        passed: 0,
        failed: 1,
        duration: const Duration(seconds: 1),
      );
      final verification = InMemoryVerificationRepository(
        gates: const {
          'ready': [analyzeGate],
        },
        history: {
          'ready': [failed],
        },
        results: const {
          'analyze': ScriptedGateResult(tests: TestCounts(passed: 1)),
        },
      );
      await tester.pumpWidget(
        _app(
          TaskBoardPanel(
            repository: InMemoryTaskRepository(tasks: const [readyTask]),
            verificationRepository: verification,
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(
        find.byKey(const Key('verification-completion-blocker')),
        findsOneWidget,
      );
      expect(
        find.textContaining('required verification gates failed'),
        findsAtLeastNWidgets(1),
      );
      expect(
        tester
            .widget<FilledButton>(find.byKey(const Key('complete-task')))
            .onPressed,
        isNull,
      );

      await tester.ensureVisible(find.byKey(const Key('run-verification')));
      await tester.tap(find.byKey(const Key('run-verification')));
      await tester.pumpAndSettle();

      expect(
        find.byKey(const Key('verification-completion-blocker')),
        findsNothing,
      );
      expect(
        tester
            .widget<FilledButton>(find.byKey(const Key('complete-task')))
            .onPressed,
        isNotNull,
      );
    },
  );
}

const taskId = 'phase-22';
const _commands = [
  ProjectCommand(
    id: 'analyze',
    label: 'Static analysis',
    command: 'flutter analyze',
  ),
  ProjectCommand(id: 'test', label: 'Tests', command: 'flutter test'),
];
const analyzeGate = VerificationGate(
  id: 'analyze',
  label: 'Static analysis',
  command: 'flutter analyze',
);
const optionalTestGate = VerificationGate(
  id: 'test',
  label: 'Tests',
  command: 'flutter test',
  required: false,
);
const readyTask = RoadmapTask(
  id: 'ready',
  title: 'Verification UX',
  phase: 22,
  status: TaskStatus.review,
  assignee: 'Desktop agent',
  planApproved: true,
  steps: [
    PlanStep(id: 'done', title: 'Implement', status: PlanStepStatus.complete),
  ],
  criteria: [
    AcceptanceCriterion(
      id: 'accepted',
      title: 'Tests pass',
      status: CriterionStatus.passed,
    ),
  ],
);

VerificationRun _run({
  required String id,
  required VerificationRunStatus status,
  required int passed,
  required int failed,
  required Duration duration,
  String stdout = '',
}) {
  final started = DateTime(2026, 7, 16, 12);
  return VerificationRun(
    id: id,
    taskId: taskId,
    status: status,
    startedAt: started,
    completedAt: started.add(duration),
    gates: [
      GateExecution(
        gateId: 'analyze',
        label: 'Static analysis',
        required: true,
        status: status == VerificationRunStatus.passed
            ? GateStatus.passed
            : GateStatus.failed,
        duration: duration,
        tests: TestCounts(passed: passed, failed: failed),
        stdout: stdout,
        fileLinks: const [
          VerificationFileLink(path: 'test/app_test.dart', line: 8),
        ],
      ),
    ],
  );
}

Future<void> _flushEvents() async {
  for (var index = 0; index < 6; index++) {
    await Future<void>.delayed(Duration.zero);
  }
}

Widget _app(Widget child) => MaterialApp(
  theme: buildLunaDarkTheme(),
  home: Scaffold(body: child),
);

Future<void> _setSize(WidgetTester tester, Size size) async {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = size;
  addTearDown(tester.view.resetDevicePixelRatio);
  addTearDown(tester.view.resetPhysicalSize);
}
