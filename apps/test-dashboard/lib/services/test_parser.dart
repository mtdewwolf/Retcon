import '../models/test_suite_models.dart';

/// Parse heterogeneous test runner output into normalized case results.
class TestOutputParser {
  const TestOutputParser();

  List<TestCaseResult> parse({
    required TestStack stack,
    required String stdout,
    required String stderr,
  }) {
    final output = stdout.trim().isEmpty ? stderr : stdout;
    if (output.trim().isEmpty) {
      return const [];
    }

    return switch (stack) {
      TestStack.rust || TestStack.protocol => _parseCargo(output),
      TestStack.flutter => _parseFlutter(output),
      TestStack.bun => _parseBun(output),
      TestStack.node => _parseNodeSummary(output),
      TestStack.powershell => const [],
    };
  }

  List<TestCaseResult> _parseCargo(String output) {
    final cases = <TestCaseResult>[];
    final failureMessages = _cargoFailureMessages(output);

    for (final line in output.split('\n')) {
      final trimmed = line.trim();
      if (!trimmed.startsWith('test ')) continue;
      final match = RegExp(
        r'^test (?<name>.+) \.\.\. (?<status>ok|FAILED|ignored)$',
      ).firstMatch(trimmed);
      if (match == null) continue;
      final name = match.namedGroup('name')!;
      final statusRaw = match.namedGroup('status')!;
      cases.add(
        TestCaseResult(
          name: name,
          status: switch (statusRaw) {
            'ok' => TestCaseStatus.passed,
            'ignored' => TestCaseStatus.ignored,
            _ => TestCaseStatus.failed,
          },
          message: statusRaw == 'FAILED' ? failureMessages[name] : null,
        ),
      );
    }
    return cases;
  }

  Map<String, String> _cargoFailureMessages(String output) {
    final messages = <String, String>{};
    final lines = output.split('\n');
    for (var i = 0; i < lines.length; i++) {
      final line = lines[i];
      final match = RegExp(
        r"^thread '(?<name>[^']+)' panicked at ",
      ).firstMatch(line);
      if (match != null) {
        final name = match.namedGroup('name')!;
        messages[name] = line.trim();
      }
    }
    return messages;
  }

  List<TestCaseResult> _parseFlutter(String output) {
    final cases = <TestCaseResult>[];
    final caseLine = RegExp(
      r'^(?<duration>\d+:\d+) \+(?<passed>\d+)(?: -(?<failed>\d+))?(?: ~(?<skipped>\d+))?: (?<name>.+?)(?: \[(?<suffix>.+)\])?$',
    );

    for (final line in output.split('\n')) {
      final trimmed = line.trim();
      final match = caseLine.firstMatch(trimmed);
      if (match == null) continue;
      final name = match.namedGroup('name')!.trim();
      if (name.startsWith('loading ')) continue;
      final suffix = match.namedGroup('suffix');
      final failed = int.tryParse(match.namedGroup('failed') ?? '0') ?? 0;
      final skipped = int.tryParse(match.namedGroup('skipped') ?? '0') ?? 0;

      TestCaseStatus status;
      String? message;
      if (suffix == 'E') {
        status = TestCaseStatus.failed;
        message = suffix;
      } else if (suffix == 'SKIP' || skipped > 0) {
        status = TestCaseStatus.skipped;
      } else if (failed > 0) {
        status = TestCaseStatus.failed;
      } else {
        status = TestCaseStatus.passed;
      }

      cases.add(TestCaseResult(name: name, status: status, message: message));
    }
    return cases;
  }

  List<TestCaseResult> _parseBun(String output) {
    final cases = <TestCaseResult>[];

    // Bun/Jest-style: ✓ name (Nms) or ✗ name
    final symbolLine = RegExp(
      r'^(?<mark>[✓✗×])\s+(?<name>.+?)(?:\s+\((?<ms>\d+)ms\))?$',
    );
    for (final line in output.split('\n')) {
      final match = symbolLine.firstMatch(line.trim());
      if (match == null) continue;
      final mark = match.namedGroup('mark')!;
      cases.add(
        TestCaseResult(
          name: match.namedGroup('name')!.trim(),
          status: mark == '✓' ? TestCaseStatus.passed : TestCaseStatus.failed,
          durationMs: int.tryParse(match.namedGroup('ms') ?? ''),
        ),
      );
    }

    if (cases.isNotEmpty) {
      return cases;
    }

    // Fallback: (pass) name / (fail) name
    final parenLine = RegExp(r'^\((?<status>pass|fail)\)\s+(?<name>.+)$');
    for (final line in output.split('\n')) {
      final match = parenLine.firstMatch(line.trim());
      if (match == null) continue;
      cases.add(
        TestCaseResult(
          name: match.namedGroup('name')!.trim(),
          status: match.namedGroup('status') == 'pass'
              ? TestCaseStatus.passed
              : TestCaseStatus.failed,
        ),
      );
    }
    return cases;
  }

  List<TestCaseResult> _parseNodeSummary(String output) {
    if (output.toLowerCase().contains('error') ||
        output.toLowerCase().contains('fail')) {
      return [
        const TestCaseResult(
          name: 'website build verify',
          status: TestCaseStatus.failed,
          message: 'Build verification reported errors',
        ),
      ];
    }
    if (output.trim().isNotEmpty) {
      return [
        const TestCaseResult(
          name: 'website build verify',
          status: TestCaseStatus.passed,
        ),
      ];
    }
    return const [];
  }
}
