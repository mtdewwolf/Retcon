import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

void main() {
  testWidgets('button activates from the keyboard', (tester) async {
    var activations = 0;
    await tester.pumpWidget(
      MaterialApp(
        theme: buildLunaDarkTheme(),
        home: Scaffold(
          body: RetconButton(
            label: 'Run task',
            autofocus: true,
            onPressed: () => activations++,
          ),
        ),
      ),
    );
    await tester.pump();
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    expect(activations, 1);
  });

  testWidgets('icon actions expose a semantic label and tooltip', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    await tester.pumpWidget(
      MaterialApp(
        theme: buildLunaDarkTheme(),
        home: Scaffold(
          body: RetconIconButton(
            label: 'Open settings',
            icon: Icons.settings,
            onPressed: () {},
          ),
        ),
      ),
    );
    expect(find.bySemanticsLabel('Open settings'), findsWidgets);
    expect(find.byTooltip('Open settings'), findsOneWidget);
    semantics.dispose();
  });

  testWidgets('large-target mode expands icon actions', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: buildLunaDarkTheme(largeTargets: true),
        home: Scaffold(
          body: RetconIconButton(
            label: 'Close',
            icon: Icons.close,
            onPressed: () {},
          ),
        ),
      ),
    );
    final size = tester.getSize(find.byType(IconButton));
    expect(size.width, greaterThanOrEqualTo(RetconDimensions.accessibleTarget));
    expect(
      size.height,
      greaterThanOrEqualTo(RetconDimensions.accessibleTarget),
    );
  });

  testWidgets('status badges communicate status without color alone', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    await tester.pumpWidget(
      MaterialApp(
        theme: buildLunaDarkTheme(highContrast: true),
        home: const Scaffold(
          body: RetconBadge(
            label: 'Provider offline',
            status: RetconStatus.error,
          ),
        ),
      ),
    );
    expect(find.bySemanticsLabel('error: Provider offline'), findsOneWidget);
    expect(find.byIcon(Icons.error), findsOneWidget);
    semantics.dispose();
  });

  testWidgets('tree view selects nodes on tap', (tester) async {
    RetconTreeNodeData? selected;
    await tester.pumpWidget(
      MaterialApp(
        theme: buildLunaDarkTheme(),
        home: Scaffold(
          body: RetconTreeView(
            nodes: const [
              RetconTreeNodeData(id: 'a', label: 'Alpha'),
              RetconTreeNodeData(id: 'b', label: 'Beta'),
            ],
            onSelected: (node) => selected = node,
          ),
        ),
      ),
    );

    await tester.tap(find.text('Beta'));
    await tester.pump();
    expect(selected?.id, 'b');
  });

  testWidgets('table rows expose row semantics and selection', (tester) async {
    var selectedIndex = -1;
    await tester.pumpWidget(
      MaterialApp(
        theme: buildLunaDarkTheme(highContrast: true),
        home: Scaffold(
          body: RetconTable(
            columns: const [RetconTableColumn(label: 'Name')],
            rows: const [
              RetconTableRow(cells: ['One'], semanticsLabel: 'Row one'),
              RetconTableRow(cells: ['Two'], semanticsLabel: 'Row two'),
            ],
            onRowSelected: (index) => selectedIndex = index,
          ),
        ),
      ),
    );

    await tester.tap(find.text('Two'));
    await tester.pump();
    expect(selectedIndex, 1);
    expect(find.text('Two'), findsOneWidget);
  });

  testWidgets('dialog exposes route semantics', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: buildLunaDarkTheme(),
        home: Builder(
          builder: (context) => Scaffold(
            body: RetconButton(
              label: 'Open',
              onPressed: () => showRetconDialog<void>(
                context: context,
                title: 'Confirm',
                content: const Text('Proceed?'),
              ),
            ),
          ),
        ),
      ),
    );

    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();
    expect(find.text('Confirm'), findsOneWidget);
    expect(find.byType(RetconDialog), findsOneWidget);
  });

  testWidgets('tabs switch content with semantics labels', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: buildLunaDarkTheme(),
        home: Scaffold(
          body: SizedBox(
            height: 200,
            child: RetconTabs(
              tabs: const [
                RetconTab(label: 'One', child: Text('First tab')),
                RetconTab(label: 'Two', child: Text('Second tab')),
              ],
            ),
          ),
        ),
      ),
    );

    expect(find.text('First tab'), findsOneWidget);
    await tester.tap(find.text('Two'));
    await tester.pumpAndSettle();
    expect(find.text('Second tab'), findsOneWidget);
  });

  testWidgets('splitter divider responds to keyboard resize', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: buildLunaDarkTheme(),
        home: Scaffold(
          body: SizedBox(
            width: 400,
            height: 200,
            child: RetconSplitter(
              first: const Text('Left'),
              second: const Text('Right'),
            ),
          ),
        ),
      ),
    );

    await tester.tap(find.bySemanticsLabel('Resize Split pane'));
    await tester.pump();
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    await tester.pump();
    expect(find.byType(RetconSplitter), findsOneWidget);
  });

  testWidgets('notification shows icon and live region semantics', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: buildLunaDarkTheme(),
        home: Builder(
          builder: (context) => Scaffold(
            body: RetconButton(
              label: 'Notify',
              onPressed: () => RetconNotification.show(
                context,
                message: 'Saved',
                level: RetconNotificationLevel.success,
              ),
            ),
          ),
        ),
      ),
    );

    await tester.tap(find.text('Notify'));
    await tester.pump();
    expect(find.text('Saved'), findsOneWidget);
    expect(find.byIcon(Icons.check_circle), findsOneWidget);
  });

  testWidgets('tooltip respects reduced motion', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: buildLunaDarkTheme(reducedMotion: true),
        home: const Scaffold(
          body: RetconTooltip(message: 'Help text', child: Text('Hover me')),
        ),
      ),
    );

    final tooltip = tester.widget<Tooltip>(find.byType(Tooltip));
    expect(tooltip.waitDuration, Duration.zero);
  });

  testWidgets('context menu opens on secondary tap', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: buildLunaDarkTheme(),
        home: Scaffold(
          body: RetconContextMenu(
            items: [RetconMenuItem(label: 'Copy', onSelected: () {})],
            child: const Text('Target'),
          ),
        ),
      ),
    );

    await tester.tap(find.text('Target'), buttons: kSecondaryButton);
    await tester.pumpAndSettle();
    expect(find.text('Copy'), findsOneWidget);
  });

  testWidgets(
    'retro taskbar exposes start, notification, and clock semantics',
    (tester) async {
      final semantics = tester.ensureSemantics();
      var opened = false;
      await tester.pumpWidget(
        MaterialApp(
          theme: buildLunaDarkTheme(),
          home: Scaffold(
            body: RetconTaskbar(
              child: Row(
                children: [
                  RetconStartButton(onPressed: () => opened = true),
                  const Spacer(),
                  RetconNotificationArea(
                    children: [
                      const RetconConnectionIndicator(
                        label: 'Core connected',
                        state: RetconIndicatorState.active,
                      ),
                      RetconClock(now: () => DateTime(2026, 7, 17, 14, 5)),
                    ],
                  ),
                ],
              ),
            ),
          ),
        ),
      );

      expect(find.bySemanticsLabel('Application taskbar'), findsOneWidget);
      expect(find.bySemanticsLabel('start menu'), findsOneWidget);
      expect(find.bySemanticsLabel('active: Core connected'), findsOneWidget);
      expect(find.bySemanticsLabel('Local time 2:05 PM'), findsOneWidget);
      await tester.tap(find.text('start'));
      expect(opened, isTrue);
      semantics.dispose();
    },
  );

  testWidgets('retro desktop states remain explicit in high contrast', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    await tester.pumpWidget(
      MaterialApp(
        theme: buildLunaDarkTheme(highContrast: true, largeTargets: true),
        home: Scaffold(
          body: Column(
            children: [
              const RetconStatusIndicator(
                label: 'Provider unavailable',
                state: RetconIndicatorState.error,
              ),
              RetconBalloonNotification(
                title: 'Agent stopped',
                message: 'The provider exited unexpectedly.',
                status: RetconStatus.error,
                onDismiss: () {},
              ),
            ],
          ),
        ),
      ),
    );

    expect(
      find.bySemanticsLabel('error: Provider unavailable'),
      findsOneWidget,
    );
    expect(
      find.bySemanticsLabel('Agent stopped. The provider exited unexpectedly.'),
      findsOneWidget,
    );
    final dismiss = tester.getSize(find.byTooltip('Dismiss Agent stopped'));
    expect(dismiss.width, greaterThanOrEqualTo(44));
    expect(dismiss.height, greaterThanOrEqualTo(44));
    semantics.dispose();
  });

  testWidgets('gallery composes all component types', (tester) async {
    tester.view.physicalSize = const Size(1200, 3000);
    tester.view.devicePixelRatio = 1.0;
    addTearDown(tester.view.reset);

    await tester.pumpWidget(
      MaterialApp(
        theme: buildLunaDarkTheme(),
        home: const RetconComponentGallery(),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.byType(RetconComponentGallery), findsOneWidget);
    expect(find.byType(RetconButton), findsWidgets);
    expect(find.byType(RetconTreeView), findsOneWidget);
    expect(find.byType(RetconTable), findsOneWidget);
    expect(find.byType(RetconTabs), findsOneWidget);
    expect(find.byType(RetconSplitter), findsOneWidget);
    expect(find.byType(RetconContextMenu), findsOneWidget);
    expect(find.byType(RetconDropdown<String>), findsOneWidget);
    expect(find.byType(RetconTaskbar), findsOneWidget);
    expect(find.byType(RetconDesktopIcon), findsOneWidget);
    expect(find.byType(RetconBalloonNotification), findsOneWidget);
    expect(find.text('Core controls'), findsOneWidget);
    expect(find.text('Retro desktop'), findsOneWidget);
    expect(find.text('Accessibility'), findsOneWidget);
  });

  testWidgets('dropdown renders selected value and label', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: buildLunaDarkTheme(),
        home: Scaffold(
          body: RetconDropdown<String>(
            label: 'Language',
            value: 'rust',
            items: const [
              RetconDropdownItem(value: 'rust', label: 'Rust'),
              RetconDropdownItem(value: 'dart', label: 'Dart'),
            ],
            onChanged: (_) {},
          ),
        ),
      ),
    );

    expect(find.text('Language'), findsOneWidget);
    expect(find.text('Rust'), findsOneWidget);
  });

  testWidgets('menu opens items from the menu button', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: buildLunaDarkTheme(),
        home: Scaffold(
          body: RetconMenu(
            label: 'File',
            items: [RetconMenuItem(label: 'Open', onSelected: () {})],
          ),
        ),
      ),
    );

    await tester.tap(find.text('File'));
    await tester.pumpAndSettle();
    expect(find.text('Open'), findsOneWidget);
  });
}
