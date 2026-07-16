import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_desktop/main.dart';
import 'package:retcon_desktop/src/desktop_shell.dart';
import 'package:retcon_desktop/src/window_controller.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

void main() {
  testWidgets('desktop shell renders chrome, menus, workspace, and taskbar', (
    tester,
  ) async {
    await setDesktopSize(tester);
    await tester.pumpWidget(
      DesktopShellTestApp(
        shell: DesktopShell(
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
      expect(find.text(menu), findsOneWidget);
    }
    expect(find.text('start'), findsOneWidget);
    expect(find.text('Retcon workspace'), findsOneWidget);
  });

  testWidgets('command palette is available from Ctrl+Shift+P', (tester) async {
    await setDesktopSize(tester);
    await tester.pumpWidget(
      DesktopShellTestApp(
        shell: DesktopShell(windowController: FakeWindowController()),
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
    expect(find.text('Diagnostics'), findsOneWidget);
  });

  testWidgets('F11 routes to native fullscreen control', (tester) async {
    await setDesktopSize(tester);
    final window = FakeWindowController();
    final commands = <ShellCommand>[];
    await tester.pumpWidget(
      DesktopShellTestApp(
        shell: DesktopShell(windowController: window, onCommand: commands.add),
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
      DesktopShellTestApp(shell: DesktopShell(windowController: window)),
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
    expect(find.text('Retcon workspace'), findsOneWidget);
  });

  testWidgets('shell remains usable at the minimum window size', (
    tester,
  ) async {
    await setDesktopSize(tester, size: const Size(760, 480));
    await tester.pumpWidget(
      DesktopShellTestApp(
        shell: DesktopShell(windowController: FakeWindowController()),
      ),
    );
    expect(tester.takeException(), isNull);
    expect(find.byTooltip('Close'), findsOneWidget);
    expect(find.text('start'), findsOneWidget);
  });
}

Future<void> setDesktopSize(
  WidgetTester tester, {
  Size size = const Size(1200, 800),
}) async {
  tester.view.devicePixelRatio = 1;
  tester.view.physicalSize = size;
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
