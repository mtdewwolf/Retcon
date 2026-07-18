import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_file_viewer/retcon_file_viewer.dart';

void main() {
  test(
    'external open delegates to the injected Core-facing boundary',
    () async {
      final opened = <String>[];
      final controller = FileExplorerController(
        service: FakeFileService(),
        root: r'C:\work',
        onOpenExternal: (path) async => opened.add(path),
      );
      addTearDown(() async {
        await controller.disposeController();
        controller.dispose();
      });

      await controller.openExternal(r'C:\work\lib\main.dart');
      expect(opened, [r'C:\work\lib\main.dart']);
    },
  );

  test('file explorer source contains no direct operating-system launcher', () {
    final source = File('lib/src/file_explorer_panel.dart').readAsStringSync();
    expect(source, isNot(contains('Process.start')));
    expect(source, isNot(contains('Process.run')));
    expect(source, isNot(contains('xdg-open')));
    expect(source, isNot(contains("'/c', 'start'")));
  });
}
