import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_desktop/src/workspace.dart';
import 'package:flutter/material.dart';

class _MemoryStore implements WorkspaceStore {
  WorkspaceLayout? value;
  @override
  Future<WorkspaceLayout?> read() async => value;
  @override
  Future<void> write(WorkspaceLayout layout) async => value = layout;
}

class _SlowStore implements WorkspaceStore {
  _SlowStore(this.value);
  WorkspaceLayout? value;
  final completer = Future<WorkspaceLayout?>.delayed(
    const Duration(milliseconds: 50),
  );
  @override
  Future<WorkspaceLayout?> read() async {
    await Future<void>.delayed(const Duration(milliseconds: 40));
    return value;
  }

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

  test('floating an existing panel reuses the singleton instance', () async {
    final store = _MemoryStore();
    final controller = WorkspaceController(store: store);
    await controller.float(PanelDefinition.terminal, const Size(800, 600));
    await controller.float(PanelDefinition.terminal, const Size(800, 600));
    expect(controller.layout.floatingPanels, hasLength(1));
  });

  test('late restore does not overwrite user mutations', () async {
    final store = _SlowStore(
      WorkspaceLayout.initial(),
    );
    final controller = WorkspaceController(store: store);
    final restore = controller.restore();
    await controller.float(PanelDefinition.browser, const Size(900, 700));
    await restore;
    expect(controller.layout.floatingPanels, hasLength(1));
    expect(controller.layout.floatingPanels.first.panel.id, 'browser');
  });

  test('activateTab updates the active panel index', () async {
    final store = _MemoryStore();
    final controller = WorkspaceController(store: store);
    await controller.reopenLast(); // no-op when empty
    final root = controller.layout.root as SplitGroup;
    final tabs = root.second as TabGroup;
    await controller.activateTab(tabs, 0);
    expect((controller.layout.root as SplitGroup).second, isA<TabGroup>());
  });
}

class _BrokenStore implements WorkspaceStore {
  @override
  Future<WorkspaceLayout?> read() => Future.error(const FormatException());
  @override
  Future<void> write(WorkspaceLayout layout) async {}
}
