import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_desktop/main.dart';
import 'package:retcon_desktop/src/browser/browser.dart';
import 'package:retcon_desktop/src/core_client.dart';
import 'package:retcon_desktop/src/dev_server/dev_server.dart';
import 'package:retcon_desktop/src/desktop_shell.dart';
import 'package:retcon_desktop/src/tasks/tasks.dart';
import 'package:retcon_desktop/src/verification/verification.dart';
import 'package:retcon_desktop/src/window_controller.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

CoreClient? _sharedCore;

CoreClient testCore() =>
    _sharedCore ??= CoreClient(dataDirectory: Directory.systemTemp);

void main() {
  tearDownAll(() {
    _sharedCore?.dispose();
    _sharedCore = null;
  });

  testWidgets('desktop shell renders chrome, menus, workspace, and taskbar', (
    tester,
  ) async {
    await setDesktopSize(tester);
    await tester.pumpWidget(
      DesktopShellTestApp(
        shell: DesktopShell(
          core: testCore(),
          projectTitle: 'Retcon',
          branch: 'Master',
          provider: 'Codex connected',
          windowController: FakeWindowController(),
        ),
      ),
    );

    expect(find.text('Retcon — Retcon'), findsOneWidget);
    expect(find.text('Master'), findsOneWidget);
    expect(find.text('Codex connected'), findsOneWidget);
    for (final menu in [
      'File',
      'Edit',
      'View',
      'Agents',
      'Git',
      'Browser',
      'Tools',
      'Help',
    ]) {
      expect(find.text(menu), findsAtLeastNWidgets(1));
    }
    expect(find.text('start'), findsOneWidget);
    expect(
      find.text('Send a message to start an agent session.'),
      findsOneWidget,
    );
  });

  testWidgets('command palette is available from Ctrl+Shift+P', (tester) async {
    await setDesktopSize(tester);
    await tester.pumpWidget(
      DesktopShellTestApp(
        shell: DesktopShell(
          core: testCore(),
          windowController: FakeWindowController(),
        ),
      ),
    );
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyP);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pumpAndSettle();

    expect(find.text('Search commands'), findsOneWidget);
    expect(find.text('Open terminal'), findsOneWidget);
    expect(find.text('Approval center'), findsOneWidget);
  });

  testWidgets('Start menu is keyboard reachable and exposes primary commands', (
    tester,
  ) async {
    await setDesktopSize(tester);
    await tester.pumpWidget(
      DesktopShellTestApp(
        shell: DesktopShell(
          core: testCore(),
          windowController: FakeWindowController(),
        ),
      ),
    );

    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyS);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pumpAndSettle();

    expect(find.bySemanticsLabel('Start menu'), findsWidgets);
    for (final command in [
      'New project',
      'Open project',
      'Approval center',
      'Open terminal',
      'Open browser',
      'Task board',
      'Settings',
      'Diagnostics',
      'Exit Retcon',
    ]) {
      expect(find.text(command), findsOneWidget);
    }
    final startFocusNodes = tester.widgetList<Focus>(
      find.ancestor(of: find.text('New project'), matching: find.byType(Focus)),
    );
    expect(startFocusNodes.any((focus) => focus.autofocus), isTrue);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pumpAndSettle();
    expect(find.text('New project'), findsNothing);
  });

  test(
    'shell state tracks counters and color-independent activity events',
    () async {
      final core = FakeConnectedCoreClient();
      final state = ShellState(core);
      addTearDown(state.dispose);
      addTearDown(core.dispose);

      await state.refresh();
      expect(state.provider, 'Claude Code 2.1.211 (ready)');
      expect(state.approvalCount, 2);
      expect(state.errorCount, 1);

      core.emit('turn.started');
      core.emit('terminal.started');
      core.emit('browser.session.started');
      await Future<void>.delayed(Duration.zero);
      expect(state.agentActive, isTrue);
      expect(state.terminalActive, isTrue);
      expect(state.browserActive, isTrue);

      core.emit('session.completed');
      core.emit('terminal.exited');
      core.emit('browser.session.crashed');
      await Future<void>.delayed(Duration.zero);
      expect(state.agentActive, isFalse);
      expect(state.terminalActive, isFalse);
      expect(state.browserActive, isFalse);
    },
  );

  testWidgets('task board command opens the Phase 21 board', (tester) async {
    await setDesktopSize(tester);
    await tester.pumpWidget(
      DesktopShellTestApp(
        shell: DesktopShell(
          core: testCore(),
          taskRepository: InMemoryTaskRepository(
            tasks: const [
              RoadmapTask(
                id: 'shell-task',
                title: 'Shell task board integration',
                status: TaskStatus.planned,
              ),
            ],
          ),
          verificationRepository: InMemoryVerificationRepository(
            gates: const {
              'shell-task': [
                VerificationGate(
                  id: 'analyze',
                  label: 'Static analysis',
                  command: 'flutter analyze',
                ),
              ],
            },
          ),
          windowController: FakeWindowController(),
        ),
      ),
    );

    await tester.tap(find.text('View'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Task board').last);
    await tester.pumpAndSettle();

    expect(find.text('Shell task board integration'), findsAtLeastNWidgets(1));
    expect(find.text('Verification gates'), findsAtLeastNWidgets(1));
    expect(find.byTooltip('Close task board'), findsOneWidget);
  });

  testWidgets('connected core uses RPC task repository instead of demo data', (
    tester,
  ) async {
    await setDesktopSize(tester);
    final core = FakeConnectedCoreClient();
    addTearDown(core.dispose);
    await tester.pumpWidget(
      DesktopShellTestApp(
        shell: DesktopShell(
          core: core,
          windowController: FakeWindowController(),
        ),
      ),
    );

    await tester.tap(find.text('View'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Task board').last);
    await tester.pumpAndSettle();

    expect(find.text('No tasks match this view.'), findsOneWidget);
    expect(find.text('Evidence-based task completion'), findsNothing);
    expect(core.methods, contains('task.list'));
  });

  testWidgets('server center opens from the shell and launches preview', (
    tester,
  ) async {
    await setDesktopSize(tester);
    final devServers = InMemoryDevServerRepository(
      configs: const {
        'local-project': DevServerConfig(
          projectId: 'local-project',
          framework: 'Vite',
          startupCommand: 'npm run dev -- --port {port}',
          port: 5173,
          worktreePath: r'C:\projects\preview',
        ),
      },
    );
    await tester.pumpWidget(
      DesktopShellTestApp(
        shell: DesktopShell(
          core: testCore(),
          devServerRepository: devServers,
          windowController: FakeWindowController(),
        ),
      ),
    );

    await tester.tap(find.text('Browser').first);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Dev server center').last);
    await tester.pumpAndSettle();

    expect(find.text('Vite'), findsOneWidget);
    expect(find.text('http://127.0.0.1:5173'), findsAtLeastNWidgets(1));
    await tester.tap(find.byKey(const Key('server-start')));
    await tester.pump();
    await tester.tap(find.byKey(const Key('server-open-preview')));
    await tester.pump();
    expect(devServers.openedPreviews, ['http://127.0.0.1:5173']);
  });

  testWidgets('dev preview opens shared browser with inspection metadata', (
    tester,
  ) async {
    await setDesktopSize(tester);
    final browser = InMemoryBrowserRepository();
    final devServers = InMemoryDevServerRepository(
      configs: const {
        'local-project': DevServerConfig(
          projectId: 'local-project',
          framework: 'Vite',
          startupCommand: 'npm run dev',
          port: 5173,
          worktreePath: r'C:\projects\preview',
        ),
      },
    );
    await tester.pumpWidget(
      DesktopShellTestApp(
        shell: DesktopShell(
          core: testCore(),
          browserRepository: browser,
          devServerRepository: devServers,
          windowController: FakeWindowController(),
        ),
      ),
    );

    await tester.tap(find.text('Browser').first);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Dev server center').last);
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const Key('server-start')));
    await tester.pump();
    await tester.tap(find.byKey(const Key('server-open-preview')));
    await tester.pumpAndSettle();

    final snapshot = await browser.load();
    expect(snapshot.session!.activeTab!.url, 'http://127.0.0.1:5173');
    expect(snapshot.session!.previewMetadata['source'], 'dev-server-preview');
    expect(snapshot.session!.previewMetadata['port'], 5173);
  });

  testWidgets('connected shell uses Core dev servers and browser navigation', (
    tester,
  ) async {
    await setDesktopSize(tester);
    final core = FakeConnectedCoreClient(includeDevServer: true);
    addTearDown(core.dispose);
    await tester.pumpWidget(
      DesktopShellTestApp(
        shell: DesktopShell(
          core: core,
          windowController: FakeWindowController(),
        ),
      ),
    );

    await tester.tap(find.text('Browser').first);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Dev server center').last);
    await tester.pumpAndSettle();

    expect(find.text('Core Vite'), findsOneWidget);
    expect(core.methods, contains('devServer.list'));
    await tester.tap(find.byKey(const Key('server-open-preview')));
    await tester.pumpAndSettle();

    expect(core.methods, contains('devServer.openPreview'));
    expect(core.methods, contains('browser.session.start'));
    expect(
      core.requests.where(
        (request) =>
            request.method == 'browser.navigate' &&
            request.params['url'] == 'http://127.0.0.1:5173',
      ),
      hasLength(1),
    );
  });

  testWidgets('explicit dev server injection wins while core is connected', (
    tester,
  ) async {
    await setDesktopSize(tester);
    final core = FakeConnectedCoreClient(includeDevServer: true);
    final devServers = InMemoryDevServerRepository();
    addTearDown(core.dispose);
    await tester.pumpWidget(
      DesktopShellTestApp(
        shell: DesktopShell(
          core: core,
          devServerRepository: devServers,
          windowController: FakeWindowController(),
        ),
      ),
    );

    await tester.tap(find.text('Browser').first);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Dev server center').last);
    await tester.pumpAndSettle();

    expect(
      core.methods.where((method) => method.startsWith('devServer.')),
      isEmpty,
    );
  });

  testWidgets('connected core selects the RPC verification repository', (
    tester,
  ) async {
    await setDesktopSize(tester);
    final core = FakeConnectedCoreClient(includeTask: true);
    addTearDown(core.dispose);
    await tester.pumpWidget(
      DesktopShellTestApp(
        shell: DesktopShell(
          core: core,
          windowController: FakeWindowController(),
        ),
      ),
    );

    await tester.tap(find.text('View'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Task board').last);
    await tester.pumpAndSettle();

    expect(find.text('Core verification task'), findsAtLeastNWidgets(1));
    expect(core.methods, contains('verification.list'));
  });

  testWidgets('explicit verification injection wins while core is connected', (
    tester,
  ) async {
    await setDesktopSize(tester);
    final core = FakeConnectedCoreClient(includeTask: true);
    addTearDown(core.dispose);
    await tester.pumpWidget(
      DesktopShellTestApp(
        shell: DesktopShell(
          core: core,
          verificationRepository: InMemoryVerificationRepository(),
          windowController: FakeWindowController(),
        ),
      ),
    );

    await tester.tap(find.text('View'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Task board').last);
    await tester.pumpAndSettle();

    expect(
      core.methods.where((method) => method.startsWith('verification.')),
      isEmpty,
    );
  });

  testWidgets('F11 routes to native fullscreen control', (tester) async {
    await setDesktopSize(tester);
    final window = FakeWindowController();
    final commands = <ShellCommand>[];
    await tester.pumpWidget(
      DesktopShellTestApp(
        shell: DesktopShell(
          core: testCore(),
          windowController: window,
          onCommand: commands.add,
        ),
      ),
    );
    await tester.sendKeyEvent(LogicalKeyboardKey.f11);
    await tester.pump();

    expect(window.fullScreenToggles, 1);
    expect(commands, contains(ShellCommand.fullScreen));
  });

  testWidgets('window controls route through the controller', (tester) async {
    await setDesktopSize(tester);
    final window = FakeWindowController();
    await tester.pumpWidget(
      DesktopShellTestApp(
        shell: DesktopShell(core: testCore(), windowController: window),
      ),
    );
    await tester.tap(find.byTooltip('Minimize'));
    await tester.tap(find.byTooltip('Maximize or restore'));
    await tester.tap(find.byTooltip('Close'));
    await tester.pump();

    expect(window.minimizes, 1);
    expect(window.maximizeToggles, 1);
    expect(window.closes, 1);
  });

  testWidgets('application still boots through RetconApp', (tester) async {
    await setDesktopSize(tester);
    await tester.pumpWidget(const RetconApp());
    expect(
      find.text('Send a message to start an agent session.'),
      findsOneWidget,
    );
  });

  testWidgets('shell remains usable at the minimum window size', (
    tester,
  ) async {
    await setDesktopSize(tester, size: const Size(760, 480));
    await tester.pumpWidget(
      DesktopShellTestApp(
        shell: DesktopShell(
          core: testCore(),
          windowController: FakeWindowController(),
        ),
      ),
    );
    expect(tester.takeException(), isNull);
    expect(find.byTooltip('Close'), findsOneWidget);
    expect(find.text('start'), findsOneWidget);
  });

  for (final scale in const [1.25, 1.5]) {
    testWidgets('shell remains usable at ${scale}x display scaling', (
      tester,
    ) async {
      await setDesktopSize(
        tester,
        size: const Size(760, 480),
        devicePixelRatio: scale,
      );
      await tester.pumpWidget(
        DesktopShellTestApp(
          shell: DesktopShell(
            core: testCore(),
            windowController: FakeWindowController(),
          ),
        ),
      );

      expect(tester.takeException(), isNull);
      expect(find.byTooltip('Close'), findsOneWidget);
      expect(find.text('start'), findsOneWidget);
    });
  }
}

Future<void> setDesktopSize(
  WidgetTester tester, {
  Size size = const Size(1200, 800),
  double devicePixelRatio = 1,
}) async {
  tester.view.devicePixelRatio = devicePixelRatio;
  tester.view.physicalSize = Size(
    size.width * devicePixelRatio,
    size.height * devicePixelRatio,
  );
  addTearDown(tester.view.resetDevicePixelRatio);
  addTearDown(tester.view.resetPhysicalSize);
}

class DesktopShellTestApp extends StatelessWidget {
  const DesktopShellTestApp({required this.shell, super.key});
  final Widget shell;

  @override
  Widget build(BuildContext context) =>
      MaterialApp(theme: buildLunaDarkTheme(), home: shell);
}

class FakeWindowController implements RetconWindowController {
  int minimizes = 0;
  int maximizeToggles = 0;
  int closes = 0;
  int fullScreenToggles = 0;

  @override
  Future<void> minimize() async => minimizes++;

  @override
  Future<void> toggleMaximized() async => maximizeToggles++;

  @override
  Future<void> close() async => closes++;

  @override
  Future<void> toggleFullScreen() async => fullScreenToggles++;
}

class FakeConnectedCoreClient extends CoreClient {
  FakeConnectedCoreClient({
    this.includeTask = false,
    this.includeDevServer = false,
  });

  final bool includeTask;
  final bool includeDevServer;
  final methods = <String>[];
  final requests = <_CoreRequest>[];
  final _fakeEvents = StreamController<Map<String, dynamic>>.broadcast();
  bool _browserRunning = false;

  void emit(String kind) => _fakeEvents.add({'kind': kind});

  @override
  CoreConnectionStatus get status => CoreConnectionStatus.connected;

  @override
  Stream<Map<String, dynamic>> get events => _fakeEvents.stream;

  @override
  Future<Map<String, dynamic>> request(
    String method, {
    Map<String, dynamic> params = const {},
    Duration timeout = const Duration(seconds: 30),
  }) async {
    methods.add(method);
    requests.add(_CoreRequest(method, params));
    if (method == 'provider.doctor') {
      return const {
        'provider_name': 'Claude Code',
        'overall_status': 'ready',
        'version': '2.1.211',
        'checks': [
          {'status': 'failure'},
          {'status': 'success'},
        ],
      };
    }
    if (method == 'approval.list') {
      return const {'pendingCount': 2, 'approvals': []};
    }
    if (method == 'task.list') {
      return includeTask
          ? const {
              'tasks': [
                {'id': 'shell-core-task', 'title': 'Core verification task'},
              ],
            }
          : const {'tasks': []};
    }
    if (method == 'task.get') {
      return const {
        'task': {
          'task': {
            'id': 'shell-core-task',
            'title': 'Core verification task',
            'status': 'planned',
          },
          'steps': [],
          'acceptanceCriteria': [],
        },
      };
    }
    if (method == 'verification.list') {
      return const {'verifications': []};
    }
    if (includeDevServer) {
      if (method == 'devServer.list') {
        return const {
          'configs': [_shellDevServerConfig],
          'instances': [_shellDevServerInstance],
        };
      }
      if (method == 'devServer.history') {
        return const {
          'events': [
            {'kind': 'started', 'actor': 'local_user', 'createdAt': 2000},
          ],
        };
      }
      if (method == 'devServer.logs') {
        return const {
          'log': {'text': 'ready\n'},
        };
      }
      if (method == 'devServer.openPreview') {
        return const {
          'instanceId': '33333333-3333-4333-8333-333333333333',
          'status': 'running',
          'url': 'http://127.0.0.1:5173',
          'port': 5173,
          'preview': {'title': 'Core preview'},
        };
      }
      if (method == 'browser.session.list') {
        return {
          'sessions': _browserRunning ? [_shellBrowserSession] : <Object>[],
        };
      }
      if (method == 'browser.session.start') {
        _browserRunning = true;
        return const {
          'session': _shellBrowserSession,
          'initialTab': _shellBrowserTab,
        };
      }
      if (method == 'browser.session.status') {
        return const {
          'session': _shellBrowserSession,
          'tabs': [_shellBrowserTab],
          'takeover': null,
        };
      }
      if (method == 'browser.session.history') return const {'events': []};
      if (method == 'browser.observation.list') {
        return const {'observations': [], 'console': [], 'network': []};
      }
      if (method == 'browser.navigate') {
        return const {
          'result': {'url': 'http://127.0.0.1:5173', 'title': 'Core preview'},
          'artifacts': [],
          'tab': _shellBrowserTab,
        };
      }
    }
    return const {};
  }

  @override
  void dispose() {
    _fakeEvents.close();
    super.dispose();
  }
}

class _CoreRequest {
  const _CoreRequest(this.method, this.params);
  final String method;
  final Map<String, dynamic> params;
}

const _shellDevServerConfig = <String, dynamic>{
  'id': '22222222-2222-4222-8222-222222222222',
  'projectId': 'local-project',
  'name': 'Core Vite',
  'command': 'npm run dev',
  'cwd': r'C:\projects\preview',
  'host': '127.0.0.1',
  'preferredPort': 5173,
  'autoStart': false,
  'environmentKeys': <String>[],
};

const _shellDevServerInstance = <String, dynamic>{
  'id': '33333333-3333-4333-8333-333333333333',
  'configId': '22222222-2222-4222-8222-222222222222',
  'projectId': 'local-project',
  'port': 5173,
  'status': 'running',
  'url': 'http://127.0.0.1:5173',
  'preview': {'title': 'Core preview'},
  'createdAt': 1000,
  'startedAt': 2000,
};

const _shellBrowserSession = <String, dynamic>{
  'id': '77777777-7777-4777-8777-777777777777',
  'projectId': 'local-project',
  'profileId': '88888888-8888-4888-8888-888888888888',
  'devServerInstanceId': '33333333-3333-4333-8333-333333333333',
  'status': 'running',
  'networkPolicy': 'loopback',
  'startedAt': 1000,
};

const _shellBrowserTab = <String, dynamic>{
  'id': '99999999-9999-4999-8999-999999999999',
  'browserSessionId': '77777777-7777-4777-8777-777777777777',
  'serviceTabId': 'service-tab',
  'url': 'http://127.0.0.1:5173',
  'title': 'Core preview',
  'status': 'open',
  'createdAt': 1000,
  'updatedAt': 1000,
};
