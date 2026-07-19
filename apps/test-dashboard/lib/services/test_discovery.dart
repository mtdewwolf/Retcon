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

/// Directories that never contain project test sources.
const _skippedDirs = {
  '.git',
  '.dart_tool',
  '.claude',
  'node_modules',
  'target',
  'build',
};

void _walk(Directory dir, void Function(File file) onFile) {
  if (!dir.existsSync()) return;
  for (final entity in dir.listSync(followLinks: false)) {
    if (entity is Directory) {
      if (_skippedDirs.contains(p.basename(entity.path))) continue;
      _walk(entity, onFile);
    } else if (entity is File) {
      onFile(entity);
    }
  }
}

final _globCache = <String, RegExp>{};

bool _matchesGlob(String relativePath, String glob) {
  final normalized = relativePath.replaceAll(r'\', '/');
  final regex = _globCache.putIfAbsent(glob, () => globToRegExp(glob));
  return regex.hasMatch(normalized);
}

/// Convert a glob pattern to a [RegExp]. Supports `**/` (any directory
/// depth, including none), `*` (any run of non-separator characters),
/// `?` (one non-separator character), and `{a,b}` alternates.
RegExp globToRegExp(String glob) {
  final pattern = glob.replaceAll(r'\', '/');
  final buffer = StringBuffer('^');
  var i = 0;
  while (i < pattern.length) {
    final char = pattern[i];
    if (pattern.startsWith('**/', i)) {
      buffer.write('(?:.*/)?');
      i += 3;
    } else if (pattern.startsWith('**', i)) {
      buffer.write('.*');
      i += 2;
    } else if (char == '*') {
      buffer.write('[^/]*');
      i += 1;
    } else if (char == '?') {
      buffer.write('[^/]');
      i += 1;
    } else if (char == '{') {
      final end = pattern.indexOf('}', i);
      if (end == -1) {
        buffer.write(RegExp.escape(char));
        i += 1;
      } else {
        final options = pattern.substring(i + 1, end).split(',');
        buffer.write('(?:${options.map(RegExp.escape).join('|')})');
        i = end + 1;
      }
    } else {
      buffer.write(RegExp.escape(char));
      i += 1;
    }
  }
  buffer.write(r'$');
  return RegExp(buffer.toString());
}
