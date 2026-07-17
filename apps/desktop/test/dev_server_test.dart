import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_desktop/src/dev_server/dev_server.dart';
import 'package:retcon_desktop/src/tasks/tasks.dart';
import 'package:retcon_desktop/src/verification/verification.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

void main() {
  group('DevServerController', () {
    test(
      'persists config, auto-starts, controls preview, and masks log growth',
      () async {
        final repository = InMemoryDevServerRepository(
          configs: {projectId: config.copyWith(autoStart: true)},
        );
        final navigated = <String>[];
        final controller = DevServerController(
          repository: repository,
          projectId: projectId,
          worktreePath: worktree,
          maxLogCharacters: 48,
          onOpenPreview: (url) async => navigated.add(url),
        );
        addTearDown(controller.dispose);

        await controller.load();

        expect(controller.running, isTrue);
        expect(controller.stdout.length, lessThanOrEqualTo(49));
        await controller.openPreview();
        expect(repository.openedPreviews, ['http://127.0.0.1:3000']);
        expect(navigated, ['http://127.0.0.1:3000']);

        await controller.setAutoStart(false);
        await controller.setRequiredForPreview(true);
        await controller.saveEnvironment(const [
          DevServerEnvironmentVariable(
            key: 'API_TOKEN',
            value: 'top-secret',
            secret: true,
          ),
        ]);
        await controller.updateStartupCommand(
          'npm run preview -- --port {port}',
        );
        await controller.changePort(4100, restartIfRunning: true);

        expect(controller.config?.autoStart, isFalse);
        expect(controller.config?.requiredForPreview, isTrue);
        expect(controller.config?.environment.single.secret, isTrue);
        expect(controller.config?.port, 4100);
        expect(controller.running, isTrue);
        await controller.stop();
        expect(controller.status, DevServerStatus.stopped);
      },
    );

    test(
      'surfaces conflict, alternate port, crash, and startup failure',
      () async {
        final repository = InMemoryDevServerRepository(
          configs: {projectId: config},
          occupiedPorts: const {3000, 3001},
        );
        final controller = DevServerController(
          repository: repository,
          projectId: projectId,
          worktreePath: worktree,
        );
        addTearDown(controller.dispose);
        await controller.load();

        await controller.start();
        expect(controller.status, DevServerStatus.portConflict);
        expect(controller.snapshot?.suggestedPort, 3002);

        await controller.useSuggestedPort();
        expect(controller.running, isTrue);
        expect(controller.config?.port, 3002);

        repository.simulateCrash(
          projectId,
          message: 'Process exited with 137.',
        );
        await Future<void>.delayed(Duration.zero);
        expect(controller.status, DevServerStatus.crashed);
        expect(controller.stderr, contains('137'));

        final failingRepository = InMemoryDevServerRepository(
          configs: {projectId: config},
          failStartup: true,
        );
        final failing = DevServerController(
          repository: failingRepository,
          projectId: projectId,
          worktreePath: worktree,
        );
        addTearDown(failing.dispose);
        await failing.load();
        await failing.start();
        expect(failing.status, DevServerStatus.startupFailed);
        expect(failing.stderr, contains('failed during startup'));
      },
    );
  });

  testWidgets(
    'server center manages conflicts, logs, and masked environment values',
    (tester) async {
      await _setSize(tester, const Size(1200, 820));
      final repository = InMemoryDevServerRepository(
        configs: {
          projectId: config.copyWith(
            requiredForPreview: true,
            environment: const [
              DevServerEnvironmentVariable(
                key: 'API_TOKEN',
                value: 'top-secret',
                secret: true,
              ),
            ],
          ),
        },
        occupiedPorts: const {3000},
      );
      final controller = DevServerController(
        repository: repository,
        projectId: projectId,
        worktreePath: worktree,
      );
      addTearDown(controller.dispose);
      await controller.load();

      await tester.pumpWidget(_app(DevServerPanel(controller: controller)));
      expect(find.text('Next.js'), findsOneWidget);
      expect(find.text(worktree), findsOneWidget);

      await tester.tap(find.byKey(const Key('server-start')));
      await tester.pump();
      expect(find.text('Port 3000 is already in use.'), findsOneWidget);
      expect(find.text('Use port 3001'), findsOneWidget);

      await tester.tap(find.byKey(const Key('server-use-alternate-port')));
      await tester.pump();
      expect(find.text('Running'), findsAtLeastNWidgets(1));

      await tester.tap(find.text('Live logs'));
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('server-stdout')), findsOneWidget);
      expect(find.textContaining('Ready on'), findsOneWidget);

      await tester.tap(find.text('Environment'));
      await tester.pumpAndSettle();
      expect(find.text('API_TOKEN'), findsOneWidget);
      expect(find.text('••••••••'), findsOneWidget);
      expect(find.text('top-secret'), findsNothing);
    },
  );

  testWidgets(
    'task preview action never satisfies a failed verification gate',
    (tester) async {
      await _setSize(tester, const Size(1200, 900));
      final devServers = InMemoryDevServerRepository(
        configs: {
          'local-project': config.copyWith(
            projectId: 'local-project',
            requiredForPreview: true,
          ),
        },
      );
      final verification = InMemoryVerificationRepository(
        gates: const {
          'ready': [verificationGate],
        },
        history: {
          'ready': [failedRun],
        },
      );
      await tester.pumpWidget(
        _app(
          TaskBoardPanel(
            repository: InMemoryTaskRepository(tasks: const [readyTask]),
            verificationRepository: verification,
            devServerRepository: devServers,
          ),
        ),
      );
      await tester.pumpAndSettle();

      await tester.ensureVisible(
        find.byKey(const Key('verification-open-preview')),
      );
      await tester.tap(find.byKey(const Key('verification-open-preview')));
      await tester.pumpAndSettle();

      expect(devServers.openedPreviews, ['http://127.0.0.1:3000']);
      expect(
        tester
            .widget<FilledButton>(find.byKey(const Key('complete-task')))
            .onPressed,
        isNull,
      );
      expect(
        find.byKey(const Key('verification-completion-blocker')),
        findsOneWidget,
      );
      await tester.ensureVisible(
        find.byKey(const Key('server-not-verification-evidence')),
      );
      expect(
        find.byKey(const Key('server-not-verification-evidence')),
        findsOneWidget,
      );
    },
  );
}

const projectId = 'phase-23-project';
const worktree = r'C:\worktrees\phase-23';
const config = DevServerConfig(
  projectId: projectId,
  framework: 'Next.js',
  startupCommand:
      'npm run dev -- --port {port} --verbose-with-enough-output-to-bound',
  port: 3000,
  worktreePath: worktree,
);
const verificationGate = VerificationGate(
  id: 'browser-check',
  label: 'Browser check',
  command: 'flutter test',
);
final failedRun = VerificationRun(
  id: 'failed-run',
  taskId: 'ready',
  status: VerificationRunStatus.failed,
  startedAt: DateTime(2026, 7, 16),
  completedAt: DateTime(2026, 7, 16, 0, 0, 1),
  gates: const [
    GateExecution(
      gateId: 'browser-check',
      label: 'Browser check',
      required: true,
      status: GateStatus.failed,
    ),
  ],
);
const readyTask = RoadmapTask(
  id: 'ready',
  title: 'Preview without false success',
  status: TaskStatus.review,
  planApproved: true,
  steps: [
    PlanStep(id: 'done', title: 'Implement', status: PlanStepStatus.complete),
  ],
  criteria: [
    AcceptanceCriterion(
      id: 'accepted',
      title: 'UX accepted',
      status: CriterionStatus.passed,
    ),
  ],
);

Widget _app(Widget child) => MaterialApp(
  theme: buildLunaDarkTheme(),
  home: Scaffold(body: child),
);

Future<void> _setSize(WidgetTester tester, Size size) async {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = size;
  addTearDown(tester.view.reset);
}
