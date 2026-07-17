import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_desktop/src/tasks/tasks.dart';
import 'package:retcon_desktop/src/verification/verification.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

void main() {
  test(
    'browser controller exposes blocking evidence and approves baselines',
    () async {
      final repository = _repository();
      final controller = BrowserVerificationController(
        repository: repository,
        taskId: taskId,
      );
      addTearDown(controller.dispose);

      await controller.load();
      expect(controller.required, isTrue);
      expect(controller.allowsCompletion, isFalse);
      expect(controller.completionBlocker, contains('Run'));

      await controller.run();
      await Future<void>.delayed(Duration.zero);

      expect(controller.latestRun, isNotNull);
      expect(
        controller.latestRun!.timeline.map((event) => event.kind),
        contains(BrowserTimelineKind.console),
      );
      expect(controller.completionBlocker, contains('accessibility'));
      expect(controller.warningCount, 2);

      await controller.approveBaseline(
        controller.latestRun!.visualComparisons.single,
      );
      expect(
        controller.latestRun!.visualComparisons.single.status,
        VisualComparisonStatus.approved,
      );
    },
  );

  test(
    'browser controller validates assertion expectations before saving',
    () async {
      final repository = _repository();
      final controller = BrowserVerificationController(
        repository: repository,
        taskId: taskId,
      );
      addTearDown(controller.dispose);
      await controller.load();

      await controller.saveDefinition(
        definition.copyWith(
          steps: const [
            BrowserVerificationStep(
              id: 'status',
              label: 'Check response',
              kind: BrowserStepKind.navigate,
              target: '/api/status',
              assertions: [
                BrowserAssertion(
                  id: 'status-code',
                  kind: BrowserAssertionKind.statusCode,
                  expected: '99',
                ),
              ],
            ),
          ],
        ),
      );

      expect(controller.error, contains('between 100 and 599'));
      expect(controller.definition?.steps.single.id, 'open');
    },
  );

  testWidgets(
    'workspace edits definitions and reviews complete browser evidence',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1080, 780));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final controller = BrowserVerificationController(
        repository: _repository(),
        taskId: taskId,
      );
      addTearDown(controller.dispose);
      await controller.load();

      await tester.pumpWidget(
        _app(BrowserVerificationPanel(controller: controller)),
      );
      await tester.pumpAndSettle();

      expect(find.byKey(const Key('browser-target-url')), findsOneWidget);
      expect(find.text('Desktop · 1440x900 @ 1.0x'), findsOneWidget);
      expect(find.byKey(const Key('browser-step-open')), findsOneWidget);
      expect(find.text('Dashboard'), findsWidgets);

      await tester.tap(find.byKey(const Key('add-mobile-viewport')));
      await tester.pump();
      expect(find.textContaining('Mobile · 390x844'), findsOneWidget);

      await tester.tap(find.byKey(const Key('run-browser-verification')));
      await tester.pumpAndSettle();
      expect(find.text('Failed'), findsOneWidget);

      await tester.tap(find.text('Timeline'));
      await tester.pumpAndSettle();
      expect(
        find.byKey(const Key('browser-verification-timeline')),
        findsOneWidget,
      );
      expect(find.text('Console error detected'), findsOneWidget);
      expect(find.text('User takeover started'), findsOneWidget);

      await tester.tap(find.text('Visual'));
      await tester.pumpAndSettle();
      expect(find.text('Baseline'), findsOneWidget);
      expect(find.text('Current'), findsOneWidget);
      expect(find.text('Difference'), findsOneWidget);
      expect(find.textContaining('3.00% difference'), findsOneWidget);
      expect(
        find.textContaining('Ignored dynamic regions: Clock'),
        findsOneWidget,
      );
      await tester.tap(find.byKey(const Key('approve-baseline-comparison')));
      await tester.pumpAndSettle();
      expect(find.text('Baseline approved'), findsOneWidget);

      await tester.tap(find.text('Accessibility'));
      await tester.pumpAndSettle();
      expect(find.text('Missing labels (1)'), findsOneWidget);
      expect(find.text('Contrast (1)'), findsOneWidget);
      expect(find.text('Save button has no accessible label'), findsOneWidget);

      await tester.tap(find.text('Evidence'));
      await tester.pumpAndSettle();
      expect(
        find.byKey(const Key('browser-completion-blocker')),
        findsOneWidget,
      );
      expect(find.byKey(const Key('browser-console-error')), findsOneWidget);
      expect(find.text('Uncaught TypeError in dashboard'), findsOneWidget);
    },
  );

  testWidgets('workspace remains usable at a narrow desktop panel width', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(620, 720));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final controller = BrowserVerificationController(
      repository: _repository(),
      taskId: taskId,
    );
    addTearDown(controller.dispose);
    await controller.load();

    await tester.pumpWidget(
      _app(BrowserVerificationPanel(controller: controller)),
    );
    await tester.pumpAndSettle();

    expect(
      find.byKey(const Key('browser-verification-definition')),
      findsOneWidget,
    );
    expect(tester.takeException(), isNull);
    await tester.tap(find.text('Accessibility'));
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
  });

  testWidgets('critical browser evidence blocks task completion', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 900));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final browserRun = _failedRun;
    final browserRepository = InMemoryBrowserVerificationRepository(
      definitions: {taskId: definition},
      history: {
        taskId: [browserRun],
      },
    );
    final verificationRepository = InMemoryVerificationRepository(
      gates: const {
        taskId: [commandGate],
      },
      history: {
        taskId: [commandRun],
      },
    );

    await tester.pumpWidget(
      _app(
        TaskBoardPanel(
          repository: InMemoryTaskRepository(tasks: const [readyTask]),
          verificationRepository: verificationRepository,
          browserVerificationRepository: browserRepository,
        ),
      ),
    );
    await tester.pumpAndSettle();

    await tester.drag(
      find.byKey(const Key('task-detail-scroll')),
      const Offset(0, -600),
    );
    await tester.pumpAndSettle();
    expect(
      find.byKey(const Key('browser-verification-completion-blocker')),
      findsOneWidget,
    );
    expect(
      find.byKey(const Key('task-browser-evidence-summary')),
      findsOneWidget,
    );
    expect(find.textContaining('Critical accessibility'), findsWidgets);
    await tester.drag(
      find.byKey(const Key('task-detail-scroll')),
      const Offset(0, -500),
    );
    await tester.pumpAndSettle();
    expect(
      tester
          .widget<FilledButton>(find.byKey(const Key('complete-task')))
          .onPressed,
      isNull,
    );
  });
}

Widget _app(Widget child) => MaterialApp(
  theme: buildLunaDarkTheme(),
  home: Scaffold(body: child),
);

InMemoryBrowserVerificationRepository _repository() =>
    InMemoryBrowserVerificationRepository(
      definitions: {taskId: definition},
      results: {
        definition.id: ScriptedBrowserVerificationResult(
          status: BrowserRunStatus.failed,
          timeline: _timeline,
          visualComparisons: [comparison],
          accessibilityIssues: const [criticalLabel, contrastWarning],
          consoleErrors: const ['Uncaught TypeError in dashboard'],
          warningMessages: const ['Network request took 2.4 seconds'],
        ),
      },
    );

const taskId = 'task-browser-verification';

const definition = BrowserVerificationDefinition(
  id: 'dashboard-browser-definition',
  taskId: taskId,
  name: 'Dashboard browser verification',
  targetUrl: 'http://127.0.0.1:3000/dashboard',
  requiredServerId: 'frontend',
  timeout: Duration(seconds: 25),
  retryCount: 2,
  visualThreshold: 0.01,
  maskSelectors: ['.clock'],
  ignoreSelectors: ['video'],
  steps: [
    BrowserVerificationStep(
      id: 'open',
      label: 'Open dashboard',
      kind: BrowserStepKind.navigate,
      target: '/dashboard',
      assertions: [
        BrowserAssertion(
          id: 'heading',
          kind: BrowserAssertionKind.text,
          target: 'h1',
          expected: 'Dashboard',
        ),
      ],
    ),
  ],
);

final _timeline = <BrowserTimelineEvent>[
  BrowserTimelineEvent(
    id: 'interaction',
    kind: BrowserTimelineKind.interaction,
    label: 'Clicked Save',
    createdAt: DateTime(2026, 7, 17, 12, 0, 1),
  ),
  BrowserTimelineEvent(
    id: 'network',
    kind: BrowserTimelineKind.network,
    label: 'POST /api/settings returned 200',
    createdAt: DateTime(2026, 7, 17, 12, 0, 2),
  ),
  BrowserTimelineEvent(
    id: 'console',
    kind: BrowserTimelineKind.console,
    label: 'Console error detected',
    createdAt: DateTime(2026, 7, 17, 12, 0, 3),
    passed: false,
  ),
  BrowserTimelineEvent(
    id: 'screenshot',
    kind: BrowserTimelineKind.screenshot,
    label: 'Captured desktop screenshot',
    createdAt: DateTime(2026, 7, 17, 12, 0, 4),
  ),
  BrowserTimelineEvent(
    id: 'takeover',
    kind: BrowserTimelineKind.takeover,
    label: 'User takeover started',
    createdAt: DateTime(2026, 7, 17, 12, 0, 5),
  ),
];

final comparison = BrowserVisualComparison(
  id: 'comparison',
  viewport: BrowserViewport(
    id: 'desktop',
    label: 'Desktop',
    width: 1440,
    height: 900,
  ),
  method: VisualComparisonMethod.perceptual,
  status: VisualComparisonStatus.changed,
  threshold: 0.01,
  difference: 0.03,
  createdAt: DateTime.fromMillisecondsSinceEpoch(1784290000000),
  baseline: const BrowserVisualArtifact(hash: baselineHash),
  current: const BrowserVisualArtifact(hash: currentHash),
  diff: const BrowserVisualArtifact(hash: diffHash),
  masks: const [
    VisualMaskRegion(
      id: 'clock',
      label: 'Clock',
      x: 20,
      y: 20,
      width: 120,
      height: 32,
    ),
  ],
);

const baselineHash =
    'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';
const currentHash =
    'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb';
const diffHash =
    'cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc';

const criticalLabel = BrowserAccessibilityIssue(
  id: 'missing-label',
  category: AccessibilityCategory.labels,
  severity: AccessibilitySeverity.critical,
  message: 'Save button has no accessible label',
  selector: '#save',
  help: 'Add an accessible name.',
);

const contrastWarning = BrowserAccessibilityIssue(
  id: 'contrast',
  category: AccessibilityCategory.contrast,
  severity: AccessibilitySeverity.warning,
  message: 'Muted text contrast is below 4.5:1',
  selector: '.muted',
);

final _failedRun = BrowserVerificationRun(
  id: 'failed-browser-run',
  taskId: taskId,
  definitionId: definition.id,
  status: BrowserRunStatus.failed,
  startedAt: DateTime(2026, 7, 17, 12),
  completedAt: DateTime(2026, 7, 17, 12, 0, 5),
  timeline: _timeline,
  visualComparisons: [comparison],
  accessibilityIssues: const [criticalLabel],
  consoleErrors: const ['Uncaught TypeError in dashboard'],
);

const commandGate = VerificationGate(
  id: 'tests',
  label: 'Tests',
  command: 'flutter test',
);

final commandRun = VerificationRun(
  id: 'command-run',
  taskId: taskId,
  status: VerificationRunStatus.passed,
  startedAt: DateTime.fromMillisecondsSinceEpoch(1784290000000),
  completedAt: DateTime.fromMillisecondsSinceEpoch(1784290001000),
  gates: const [
    GateExecution(
      gateId: 'tests',
      label: 'Tests',
      required: true,
      status: GateStatus.passed,
    ),
  ],
);

const readyTask = RoadmapTask(
  id: taskId,
  title: 'Verify dashboard',
  status: TaskStatus.review,
  planApproved: true,
  steps: [
    PlanStep(
      id: 'step',
      title: 'Build dashboard',
      status: PlanStepStatus.complete,
    ),
  ],
  criteria: [
    AcceptanceCriterion(
      id: 'criterion',
      title: 'Dashboard works',
      status: CriterionStatus.passed,
    ),
  ],
);
