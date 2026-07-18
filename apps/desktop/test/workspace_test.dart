import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_desktop/src/workspace.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

class _MemoryStore implements WorkspaceStore {
  WorkspaceLayout? value;
  @override
  Future<WorkspaceLayout?> read() async => value;
  @override
  Future<void> write(WorkspaceLayout layout) async => value = layout;
}

void main() {
  test('workspace layouts serialize tabs, splits, and floating panels', () {
    final layout = WorkspaceLayout.initial();
    final restored = WorkspaceLayout.fromJson(layout.toJson());

    expect(restored.version, WorkspaceLayout.currentVersion);
    expect(restored.root, isA<SplitGroup>());
    expect((restored.root as SplitGroup).first, isA<TabGroup>());
  });

  test('closed panels can be reopened after persistence', () async {
    final store = _MemoryStore();
    final controller = WorkspaceController(store: store);
    await controller.close(PanelDefinition.workspace);
    await controller.reopenLast();

    expect(store.value?.closedPanels, isEmpty);
    expect(store.value?.root.toJson().toString(), contains('workspace'));
  });

  test('invalid saved data recovers to the default layout', () async {
    final controller = WorkspaceController(store: _BrokenStore());
    await controller.restore();
    expect(controller.layout.root, isA<SplitGroup>());
  });

  test('layout presets produce distinct roots', () {
    final presets = LayoutPreset.values.map((preset) => preset.layout().root);
    expect(presets.toSet().length, LayoutPreset.values.length);
  });

  test('selectTab activates an existing panel in a tab group', () async {
    final store = _MemoryStore();
    final controller = WorkspaceController(store: store);
    await controller.applyPreset(LayoutPreset.checkpoints);
    await controller.openPanel(PanelDefinition.browser);

    final root = controller.layout.root as SplitGroup;
    final second = root.second as SplitGroup;
    final tabs = second.second as TabGroup;
    expect(tabs.panels.map((panel) => panel.id), ['checkpoints', 'browser']);
    expect(tabs.active.id, 'browser');

    await controller.selectTab('checkpoints');
    final activated =
        ((controller.layout.root as SplitGroup).second as SplitGroup).second
            as TabGroup;
    expect(activated.active.id, 'checkpoints');
  });

  test('openPanel activates a panel that is already open', () async {
    final store = _MemoryStore();
    final controller = WorkspaceController(store: store);
    await controller.applyPreset(LayoutPreset.review);
    await controller.selectTab('browser');
    await controller.openPanel(PanelDefinition.checkpoints);

    final tabs =
        ((controller.layout.root as SplitGroup).second as SplitGroup).second
            as TabGroup;
    expect(tabs.active.id, 'checkpoints');
  });

  test('monitor recovery keeps floating panels inside the viewport', () async {
    final store = _MemoryStore();
    final controller = WorkspaceController(store: store);
    await controller.float(
      PanelDefinition.terminal,
      const Size(800, 600),
      detached: true,
    );
    var floating = controller.layout.floatingPanels.first;
    await controller.moveFloating(floating, const Offset(900, 700));
    floating = controller.layout.floatingPanels.first;
    await controller.recoverMonitorLayout(const Size(800, 600));
    final rect = store.value!.floatingPanels.first.rect;
    expect(rect.right, lessThanOrEqualTo(800));
    expect(rect.bottom, lessThanOrEqualTo(600));
    expect(rect.left, greaterThanOrEqualTo(8));
    expect(rect.top, greaterThanOrEqualTo(8));
  });

  test('panels dock at every edge and merge into target tabs', () async {
    for (final position in DockPosition.values) {
      final store = _MemoryStore();
      final controller = WorkspaceController(store: store);
      await controller.openPanel(PanelDefinition.terminal);
      await controller.dockPanel(
        PanelDefinition.terminal,
        targetPanelId: PanelDefinition.workspace.id,
        position: position,
      );

      final json = controller.layout.root.toJson().toString();
      expect(json, contains('terminal'));
      expect(json, contains('workspace'));
      if (position == DockPosition.tab) {
        expect(json, contains('panels'));
      } else {
        expect(json, contains('split'));
      }
    }
  });

  test('split fractions are clamped and persisted', () async {
    final store = _MemoryStore();
    final controller = WorkspaceController(store: store);
    await controller.setSplitFraction(PanelDefinition.explorer.id, .99);

    expect((store.value!.root as SplitGroup).fraction, .85);
  });

  test('detached host opens on detach and closes on redock', () async {
    final store = _MemoryStore();
    final host = _MemoryDetachedHost();
    final controller = WorkspaceController(store: store, detachedHost: host);
    await controller.float(
      PanelDefinition.terminal,
      const Size(800, 600),
      detached: true,
    );

    expect(host.opened, ['terminal']);
    final detached = controller.layout.floatingPanels.single;
    await controller.dock(detached);
    expect(host.closed, ['terminal']);
    expect(controller.layout.floatingPanels, isEmpty);
  });

  test('restored detached panels reconnect to their native host', () async {
    final store = _MemoryStore();
    store.value = WorkspaceLayout(
      root: const TabGroup(panels: [PanelDefinition.workspace]),
      floatingPanels: const [
        FloatingPanel(
          panel: PanelDefinition.browser,
          rect: Rect.fromLTWH(40, 40, 360, 260),
          detached: true,
        ),
      ],
    );
    final host = _MemoryDetachedHost();
    final controller = WorkspaceController(store: store, detachedHost: host);
    await controller.restore();

    expect(host.opened, ['browser']);
  });

  test(
    'closing a floating panel removes it and deduplicates history',
    () async {
      final store = _MemoryStore();
      final controller = WorkspaceController(store: store);
      await controller.float(PanelDefinition.terminal, const Size(800, 600));
      await controller.close(PanelDefinition.terminal);
      await controller.close(PanelDefinition.terminal);

      expect(controller.layout.floatingPanels, isEmpty);
      expect(
        controller.layout.closedPanels.where((panel) => panel.id == 'terminal'),
        hasLength(1),
      );
    },
  );

  testWidgets('workspace split is keyboard resizable with semantics', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(900, 600);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final store = _MemoryStore()..value = WorkspaceLayout.initial();
    final controller = WorkspaceController(store: store);
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      MaterialApp(
        theme: buildLunaDarkTheme(),
        home: Scaffold(body: DockingWorkspace(controller: controller)),
      ),
    );
    await tester.pumpAndSettle();
    final before = (controller.layout.root as SplitGroup).fraction;

    await tester.tap(find.bySemanticsLabel('Resize workspace split').first);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    await tester.pumpAndSettle();

    expect(
      (controller.layout.root as SplitGroup).fraction,
      greaterThan(before),
    );
  });
}

class _BrokenStore implements WorkspaceStore {
  @override
  Future<WorkspaceLayout?> read() => Future.error(const FormatException());
  @override
  Future<void> write(WorkspaceLayout layout) async {}
}

class _MemoryDetachedHost implements DetachedWindowHost {
  final List<String> opened = [];
  final List<String> closed = [];

  @override
  Future<void> open(FloatingPanel panel) async => opened.add(panel.panel.id);

  @override
  Future<void> close(String panelId) async => closed.add(panelId);
}
