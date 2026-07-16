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
}

class _BrokenStore implements WorkspaceStore {
  @override
  Future<WorkspaceLayout?> read() => Future.error(const FormatException());
  @override
  Future<void> write(WorkspaceLayout layout) async {}
}
