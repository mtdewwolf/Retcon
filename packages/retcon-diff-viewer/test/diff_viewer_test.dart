import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_diff_viewer/retcon_diff_viewer.dart';

void main() {
  group('DiffParser', () {
    const sample = '''
diff --git a/hello.txt b/hello.txt
index 1234567..89abcde 100644
--- a/hello.txt
+++ b/hello.txt
@@ -1,2 +1,2 @@
 one
-two
+three
''';

    test('parses files and hunks', () {
      const parser = DiffParser();
      final files = parser.parse(sample);
      expect(files, hasLength(1));
      expect(files.first.displayPath, 'hello.txt');
      expect(files.first.hunks, hasLength(1));
      expect(files.first.hunks.first.lines.length, greaterThanOrEqualTo(3));
    });
  });

  group('FakeDiffService', () {
    test('records hunk patch actions', () async {
      final service = FakeDiffService(diffText: 'diff --git a/x b/x\n');
      await service.stageHunk(repo: '/repo', patch: '@@ patch');
      expect(service.lastPatch, '@@ patch');
    });
  });
}
