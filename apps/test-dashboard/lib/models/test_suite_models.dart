/// Domain models for the Retcon test dashboard.
library;

enum TestStack { rust, flutter, bun, node, protocol, powershell }

enum SuiteRunStatus { idle, running, passed, failed, skipped, error }

enum TestCaseStatus { passed, failed, skipped, ignored }

enum FocusPriority { critical, high, medium, low }

enum FocusCategory {
  failure,
  notInCi,
  manualOnly,
  neverRun,
  stale,
  coverageGap,
}

class TestSuiteDefinition {
  const TestSuiteDefinition({
    required this.id,
    required this.name,
    required this.stack,
    required this.workingDirectory,
    required this.command,
    required this.commandArgs,
    this.inCi = false,
    this.ciJob,
    this.manualOnly = false,
    this.manualReason,
    this.description,
    this.testFileGlob,
  });

  final String id;
  final String name;
  final TestStack stack;
  final String workingDirectory;
  final String command;
  final List<String> commandArgs;
  final bool inCi;
  final String? ciJob;
  final bool manualOnly;
  final String? manualReason;
  final String? description;
  final String? testFileGlob;
}

class TestCaseResult {
  const TestCaseResult({
    required this.name,
    required this.status,
    this.suite,
    this.filePath,
    this.line,
    this.durationMs,
    this.message,
  });

  final String name;
  final String? suite;
  final TestCaseStatus status;
  final String? filePath;
  final int? line;
  final int? durationMs;
  final String? message;
}

class SuiteRunResult {
  const SuiteRunResult({
    required this.suiteId,
    required this.status,
    required this.startedAt,
    this.finishedAt,
    this.exitCode,
    this.stdout = '',
    this.stderr = '',
    this.cases = const [],
    this.errorMessage,
  });

  final String suiteId;
  final SuiteRunStatus status;
  final DateTime startedAt;
  final DateTime? finishedAt;
  final int? exitCode;
  final String stdout;
  final String stderr;
  final List<TestCaseResult> cases;
  final String? errorMessage;

  int get passedCount =>
      cases.where((c) => c.status == TestCaseStatus.passed).length;

  int get failedCount =>
      cases.where((c) => c.status == TestCaseStatus.failed).length;

  int get skippedCount => cases
      .where(
        (c) =>
            c.status == TestCaseStatus.skipped ||
            c.status == TestCaseStatus.ignored,
      )
      .length;

  Duration? get duration => finishedAt?.difference(startedAt);
}

class DiscoveredTests {
  const DiscoveredTests({
    required this.suiteId,
    required this.fileCount,
    required this.files,
  });

  final String suiteId;
  final int fileCount;
  final List<String> files;
}

class FocusItem {
  const FocusItem({
    required this.priority,
    required this.category,
    required this.title,
    required this.detail,
    this.suiteId,
    this.testName,
    this.actionLabel,
  });

  final FocusPriority priority;
  final FocusCategory category;
  final String title;
  final String detail;
  final String? suiteId;
  final String? testName;
  final String? actionLabel;

  int get sortOrder => switch (priority) {
    FocusPriority.critical => 0,
    FocusPriority.high => 1,
    FocusPriority.medium => 2,
    FocusPriority.low => 3,
  };
}

class DashboardSnapshot {
  const DashboardSnapshot({
    required this.repoRoot,
    required this.suites,
    required this.discovered,
    required this.runs,
    required this.focusItems,
    this.lastFullRunAt,
  });

  final String repoRoot;
  final List<TestSuiteDefinition> suites;
  final Map<String, DiscoveredTests> discovered;
  final Map<String, SuiteRunResult> runs;
  final List<FocusItem> focusItems;
  final DateTime? lastFullRunAt;

  int get totalSuites => suites.length;

  int get suitesPassed =>
      runs.values.where((run) => run.status == SuiteRunStatus.passed).length;

  int get suitesFailed =>
      runs.values.where((run) => run.status == SuiteRunStatus.failed).length;

  int get totalTestFiles =>
      discovered.values.fold(0, (sum, item) => sum + item.fileCount);

  int get totalFailedCases =>
      runs.values.fold(0, (sum, run) => sum + run.failedCount);
}
