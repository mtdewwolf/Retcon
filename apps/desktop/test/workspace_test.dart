import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_desktop/src/workspace.dart';

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
    expect(tabs.panels.map((panel) => panel.id), [
      'checkpoints',
      'browser',
    ]);
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
}

class _BrokenStore implements WorkspaceStore {
  @override
  Future<WorkspaceLayout?> read() => Future.error(const FormatException());
  @override
  Future<void> write(WorkspaceLayout layout) async {}
}
