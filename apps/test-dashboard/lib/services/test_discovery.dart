import 'dart:io';

import 'package:path/path.dart' as p;

import '../models/test_suite_models.dart';

/// Scan the repository for test files per suite definition.
Map<String, DiscoveredTests> discoverTestFiles(
  String repoRoot,
  List<TestSuiteDefinition> suites,
) {
  final results = <String, DiscoveredTests>{};
  for (final suite in suites) {
    results[suite.id] = _discoverSuite(repoRoot, suite);
  }
  return results;
}

DiscoveredTests _discoverSuite(String repoRoot, TestSuiteDefinition suite) {
  final glob = suite.testFileGlob;
  if (glob == null) {
    return _discoverByStack(repoRoot, suite);
  }

  final baseDir = suite.workingDirectory;
  if (!Directory(baseDir).existsSync()) {
    return DiscoveredTests(suiteId: suite.id, fileCount: 0, files: const []);
  }

  final files = <String>[];
  _walk(Directory(baseDir), (file) {
    if (_matchesGlob(p.relative(file.path, from: baseDir), glob)) {
      files.add(p.relative(file.path, from: repoRoot));
    }
  });
  files.sort();
  return DiscoveredTests(
    suiteId: suite.id,
    fileCount: files.length,
    files: files,
  );
}

DiscoveredTests _discoverByStack(String repoRoot, TestSuiteDefinition suite) {
  final files = <String>[];
  switch (suite.stack) {
    case TestStack.rust:
      _walk(Directory('$repoRoot/crates'), (file) {
        if (file.path.endsWith('.rs') &&
            (file.path.contains('${p.separator}tests${p.separator}') ||
                _fileContainsTestMacro(file))) {
          files.add(p.relative(file.path, from: repoRoot));
        }
      });
      if (suite.id == 'integration-wave3') {
        _walk(Directory('$repoRoot/tests/integration'), (file) {
          if (file.path.endsWith('.rs')) {
            files.add(p.relative(file.path, from: repoRoot));
          }
        });
      }
    case TestStack.protocol:
      _walk(Directory('$repoRoot/tests/fixtures'), (file) {
        if (file.path.endsWith('.json')) {
          files.add(p.relative(file.path, from: repoRoot));
        }
      });
    case TestStack.flutter:
    case TestStack.bun:
    case TestStack.node:
    case TestStack.powershell:
      break;
  }
  files.sort();
  return DiscoveredTests(
    suiteId: suite.id,
    fileCount: files.length,
    files: files,
  );
}

bool _fileContainsTestMacro(File file) {
  try {
    final content = file.readAsStringSync();
    return content.contains('#[test]') || content.contains('#[tokio::test]');
  } on FileSystemException {
    return false;
  }
}

void _walk(Directory dir, void Function(File file) onFile) {
  if (!dir.existsSync()) return;
  for (final entity in dir.listSync(recursive: true, followLinks: false)) {
    if (entity is File) {
      onFile(entity);
    }
  }
}

bool _matchesGlob(String relativePath, String glob) {
  final normalized = relativePath.replaceAll(r'\', '/');
  final pattern = glob.replaceAll(r'\', '/');
  if (pattern.startsWith('**/')) {
    final suffix = pattern.substring(3);
    return normalized.endsWith(suffix) ||
        normalized.contains('/$suffix') ||
        normalized == suffix;
  }
  return normalized == pattern;
}
