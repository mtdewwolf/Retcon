import 'dart:async';
import 'dart:convert';
import 'dart:io';

import '../models/test_suite_models.dart';
import 'test_parser.dart';

class TestRunner {
  TestRunner({TestOutputParser? parser})
    : _parser = parser ?? const TestOutputParser();

  final TestOutputParser _parser;

  Future<SuiteRunResult> runSuite(TestSuiteDefinition suite) async {
    final startedAt = DateTime.now();
    if (suite.manualOnly) {
      return SuiteRunResult(
        suiteId: suite.id,
        status: SuiteRunStatus.skipped,
        startedAt: startedAt,
        finishedAt: DateTime.now(),
        errorMessage: suite.manualReason,
      );
    }

    if (!Directory(suite.workingDirectory).existsSync()) {
      return SuiteRunResult(
        suiteId: suite.id,
        status: SuiteRunStatus.error,
        startedAt: startedAt,
        finishedAt: DateTime.now(),
        errorMessage: 'Working directory not found: ${suite.workingDirectory}',
      );
    }

    try {
      final process = await Process.start(
        suite.command,
        suite.commandArgs,
        workingDirectory: suite.workingDirectory,
        runInShell: Platform.isWindows,
        environment: _environmentFor(suite),
      );

      const decoder = Utf8Decoder(allowMalformed: true);
      final stdoutFuture = process.stdout.transform(decoder).join();
      final stderrFuture = process.stderr.transform(decoder).join();

      final exitCode = await process.exitCode;
      final stdout = await stdoutFuture;
      final stderr = await stderrFuture;
      final cases = _parser.parse(
        stack: suite.stack,
        stdout: stdout,
        stderr: stderr,
      );
      final finishedAt = DateTime.now();

      final status = _resolveStatus(exitCode, cases);
      return SuiteRunResult(
        suiteId: suite.id,
        status: status,
        startedAt: startedAt,
        finishedAt: finishedAt,
        exitCode: exitCode,
        stdout: stdout,
        stderr: stderr,
        cases: cases,
      );
    } on ProcessException catch (error) {
      return SuiteRunResult(
        suiteId: suite.id,
        status: SuiteRunStatus.error,
        startedAt: startedAt,
        finishedAt: DateTime.now(),
        errorMessage: error.message,
      );
    }
  }

  Map<String, String> _environmentFor(TestSuiteDefinition suite) {
    final env = Map<String, String>.from(Platform.environment);
    if (suite.id == 'browser-service-e2e') {
      env['RETCON_BROWSER_E2E'] = '1';
    }
    if (suite.id == 'rust-agents-e2e') {
      env['RETCON_AGENT_E2E'] = '1';
    }
    return env;
  }

  SuiteRunStatus _resolveStatus(int exitCode, List<TestCaseResult> cases) {
    if (exitCode != 0) {
      return SuiteRunStatus.failed;
    }
    if (cases.any((c) => c.status == TestCaseStatus.failed)) {
      return SuiteRunStatus.failed;
    }
    return SuiteRunStatus.passed;
  }
}
