import 'dart:convert';
import 'dart:io';

import 'package:path/path.dart' as p;

import '../models/test_suite_models.dart';

/// Loads the canonical Retcon verification inventory used by CI.
List<TestSuiteDefinition> buildTestInventory(String repoRoot) {
  final result = Process.runSync(
    'node',
    const ['scripts/verify.mjs', '--list', '--json'],
    workingDirectory: repoRoot,
    runInShell: Platform.isWindows,
  );
  if (result.exitCode != 0) {
    throw StateError(
      'Unable to load scripts/verify.mjs inventory: ${result.stderr}',
    );
  }
  return buildTestInventoryFromJson(repoRoot, result.stdout as String);
}

/// Parses canonical inventory JSON; exposed so drift behavior can be unit tested.
List<TestSuiteDefinition> buildTestInventoryFromJson(
  String repoRoot,
  String source,
) {
  final inventory = jsonDecode(source) as Map<String, dynamic>;
  final automated = inventory['dashboardSuites'] as List<dynamic>?;
  final manual = inventory['manualGates'] as List<dynamic>?;
  if (automated == null || manual == null) {
    throw const FormatException(
      'Verification inventory must include dashboardSuites and manualGates.',
    );
  }

  return [
    for (final value in automated)
      _automatedSuite(repoRoot, value as Map<String, dynamic>),
    for (final value in manual)
      _manualSuite(repoRoot, value as Map<String, dynamic>),
  ];
}

TestSuiteDefinition _automatedSuite(
  String repoRoot,
  Map<String, dynamic> value,
) {
  final commandArgs = (value['args'] as List<dynamic>)
      .map((argument) => argument as String)
      .toList();
  if (commandArgs.isNotEmpty) {
    commandArgs[0] = p.join(repoRoot, commandArgs[0]);
  }
  return TestSuiteDefinition(
    id: value['id'] as String,
    name: value['label'] as String,
    stack: _stack(value['stack'] as String),
    workingDirectory: p.join(repoRoot, value['path'] as String),
    command: 'node',
    commandArgs: commandArgs,
    inCi: true,
    ciJob: value['ciJob'] as String,
    description: value['description'] as String?,
    testFileGlob: value['testFileGlob'] as String?,
  );
}

TestSuiteDefinition _manualSuite(String repoRoot, Map<String, dynamic> value) {
  final command = value['command'] as String;
  return TestSuiteDefinition(
    id: value['id'] as String,
    name: value['label'] as String,
    stack: _stack(value['stack'] as String),
    workingDirectory: repoRoot,
    command: 'powershell',
    commandArgs: const [],
    manualOnly: true,
    manualReason: '${value['label']} - run $command.',
    description: command,
  );
}

TestStack _stack(String value) => switch (value) {
  'rust' => TestStack.rust,
  'flutter' => TestStack.flutter,
  'bun' => TestStack.bun,
  'node' => TestStack.node,
  'protocol' => TestStack.protocol,
  'powershell' => TestStack.powershell,
  _ => throw FormatException('Unknown verification stack: $value'),
};
