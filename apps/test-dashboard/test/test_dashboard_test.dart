import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;
import 'package:retcon_test_dashboard/controllers/test_dashboard_controller.dart';
import 'package:retcon_test_dashboard/models/test_suite_models.dart';
import 'package:retcon_test_dashboard/services/test_discovery.dart';
import 'package:retcon_test_dashboard/services/test_inventory.dart';
import 'package:retcon_test_dashboard/services/test_parser.dart';

void main() {
  group('TestOutputParser', () {
    const parser = TestOutputParser();

    test('parses cargo test output', () {
      const output = '''
running 3 tests
test math::adds ... ok
test math::ignored ... ignored
test math::fails ... FAILED

thread 'math::fails' panicked at tests/math.rs:42:5:
assertion failed
''';

      final cases = parser.parse(
        stack: TestStack.rust,
        stdout: output,
        stderr: '',
      );

      expect(cases, hasLength(3));
      expect(cases[0].status, TestCaseStatus.passed);
      expect(cases[1].status, TestCaseStatus.ignored);
      expect(cases[2].status, TestCaseStatus.failed);
      expect(cases[2].message, contains('panicked'));
    });

    test('parses flutter test output', () {
      const output = '''
00:00 +0: loading test/widget_test.dart
00:01 +1: renders dashboard
00:02 +1 -1: saves task [E]
00:02 +1 -1 ~1: desktop only [SKIP]
''';

      final cases = parser.parse(
        stack: TestStack.flutter,
        stdout: output,
        stderr: '',
      );

      expect(cases, hasLength(3));
      expect(cases[0].status, TestCaseStatus.passed);
      expect(cases[1].status, TestCaseStatus.failed);
      expect(cases[2].status, TestCaseStatus.skipped);
    });
  });

  group('test discovery', () {
    test('globToRegExp matches inventory glob patterns', () {
      final dartGlob = globToRegExp('test/**/*_test.dart');
      expect(dartGlob.hasMatch('test/foo_test.dart'), isTrue);
      expect(dartGlob.hasMatch('test/nested/bar_test.dart'), isTrue);
      expect(dartGlob.hasMatch('lib/foo_test.dart'), isFalse);
      expect(dartGlob.hasMatch('test/helper.dart'), isFalse);

      final tsGlob = globToRegExp('test/**/*.test.ts');
      expect(tsGlob.hasMatch('test/rpc.test.ts'), isTrue);
      expect(tsGlob.hasMatch('test/deep/rpc.test.ts'), isTrue);
      expect(tsGlob.hasMatch('src/rpc.ts'), isFalse);

      final braceGlob = globToRegExp('**/*.{rs,toml}');
      expect(braceGlob.hasMatch('crates/core/src/lib.rs'), isTrue);
      expect(braceGlob.hasMatch('Cargo.toml'), isTrue);
      expect(braceGlob.hasMatch('crates/core/src/lib.md'), isFalse);
    });

    test('discoverTestFiles finds suite files and skips vendor dirs', () {
      final root = Directory.systemTemp.createTempSync('retcon_dashboard');
      addTearDown(() => root.deleteSync(recursive: true));
      final app = Directory(p.join(root.path, 'apps', 'demo'));
      File(
        p.join(app.path, 'test', 'unit', 'a_test.dart'),
      ).createSync(recursive: true);
      File(p.join(app.path, 'test', 'b_test.dart')).createSync(recursive: true);
      File(p.join(app.path, 'test', 'helper.dart')).createSync(recursive: true);
      File(
        p.join(app.path, 'node_modules', 'pkg', 'test', 'c_test.dart'),
      ).createSync(recursive: true);

      final suite = TestSuiteDefinition(
        id: 'demo',
        name: 'Demo',
        stack: TestStack.flutter,
        workingDirectory: app.path,
        command: 'flutter',
        commandArgs: const ['test'],
        testFileGlob: 'test/**/*_test.dart',
      );

      final discovered = discoverTestFiles(root.path, [suite]);
      final files = discovered['demo']!.files
          .map((file) => file.replaceAll(r'\', '/'))
          .toList();

      expect(files, hasLength(2));
      expect(files, contains('apps/demo/test/unit/a_test.dart'));
      expect(files, contains('apps/demo/test/b_test.dart'));
    });
  });

  group('buildTestInventory', () {
    test('loads the repository canonical inventory without drift', () {
      final repoRoot = p.normalize(p.join(Directory.current.path, '..', '..'));
      final suites = buildTestInventory(repoRoot);
      final automated = suites.where((suite) => !suite.manualOnly).toList();

      expect(automated, isNotEmpty);
      expect(
        automated,
        everyElement(
          isA<TestSuiteDefinition>().having(
            (suite) => suite.inCi,
            'inCi',
            isTrue,
          ),
        ),
      );
      expect(
        automated.where((suite) => suite.id.startsWith('flutter-')),
        hasLength(6),
      );
      expect(suites.where((suite) => suite.manualOnly), hasLength(3));
    });

    test('loads every Flutter surface and manual gate from canonical JSON', () {
      const source = '''
{
  "dashboardSuites": [
    {"id":"flutter-desktop","label":"Desktop shell","stack":"flutter","path":"apps/desktop","args":["scripts/verify.mjs","--check","flutter-desktop"],"ciJob":"Flutter (all apps/packages analyze + test)"},
    {"id":"flutter-test-dashboard","label":"Test dashboard","stack":"flutter","path":"apps/test-dashboard","args":["scripts/verify.mjs","--check","flutter-test-dashboard"],"ciJob":"Flutter (all apps/packages analyze + test)"},
    {"id":"flutter-design-system","label":"Design system","stack":"flutter","path":"packages/retcon-design-system","args":["scripts/verify.mjs","--check","flutter-design-system"],"ciJob":"Flutter (all apps/packages analyze + test)"},
    {"id":"flutter-diff-viewer","label":"Diff viewer","stack":"flutter","path":"packages/retcon-diff-viewer","args":["scripts/verify.mjs","--check","flutter-diff-viewer"],"ciJob":"Flutter (all apps/packages analyze + test)"},
    {"id":"flutter-file-viewer","label":"File viewer","stack":"flutter","path":"packages/retcon-file-viewer","args":["scripts/verify.mjs","--check","flutter-file-viewer"],"ciJob":"Flutter (all apps/packages analyze + test)"},
    {"id":"flutter-terminal-view","label":"Terminal view","stack":"flutter","path":"packages/retcon-terminal-view","args":["scripts/verify.mjs","--check","flutter-terminal-view"],"ciJob":"Flutter (all apps/packages analyze + test)"}
  ],
  "manualGates": [
    {"id":"interactive-conpty","label":"Interactive ConPTY","command":"scripts/phase2/03-terminals.ps1","stack":"powershell"}
  ]
}
''';
      final suites = buildTestInventoryFromJson(r'C:\repo\retcon', source);
      final ids = suites.map((suite) => suite.id).toSet();

      expect(ids, contains('flutter-desktop'));
      expect(ids, contains('flutter-test-dashboard'));
      expect(ids, contains('flutter-file-viewer'));
      expect(ids.where((id) => id.startsWith('flutter-')), hasLength(6));
      expect(
        suites.where((suite) => suite.id.startsWith('flutter-')),
        everyElement(
          isA<TestSuiteDefinition>()
              .having((suite) => suite.inCi, 'inCi', isTrue)
              .having(
                (suite) => suite.ciJob,
                'ciJob',
                'Flutter (all apps/packages analyze + test)',
              ),
        ),
      );
      expect(
        suites
            .firstWhere((suite) => suite.id == 'interactive-conpty')
            .manualOnly,
        isTrue,
      );
    });
  });

  group('computeFocusItems', () {
    test('prioritizes failures over never-run suites', () {
      final suites = [
        const TestSuiteDefinition(
          id: 'a',
          name: 'A',
          stack: TestStack.rust,
          workingDirectory: '.',
          command: 'cargo',
          commandArgs: ['test'],
        ),
        const TestSuiteDefinition(
          id: 'b',
          name: 'B',
          stack: TestStack.flutter,
          workingDirectory: '.',
          command: 'flutter',
          commandArgs: ['test'],
          inCi: false,
        ),
      ];

      final items = computeFocusItems(
        suites: suites,
        discovered: const {
          'a': DiscoveredTests(suiteId: 'a', fileCount: 1, files: ['a.rs']),
          'b': DiscoveredTests(suiteId: 'b', fileCount: 2, files: ['b.dart']),
        },
        runs: {
          'a': SuiteRunResult(
            suiteId: 'a',
            status: SuiteRunStatus.failed,
            startedAt: DateTime.utc(2026),
            finishedAt: DateTime.utc(2026),
            cases: [
              const TestCaseResult(
                name: 'broken test',
                status: TestCaseStatus.failed,
              ),
            ],
          ),
        },
      );

      expect(items.first.category, FocusCategory.failure);
      expect(
        items.any((item) => item.category == FocusCategory.neverRun),
        isTrue,
      );
      expect(
        items.any((item) => item.category == FocusCategory.notInCi),
        isTrue,
      );
    });
  });
}
