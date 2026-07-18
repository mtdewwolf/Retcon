import 'package:flutter/foundation.dart';

import '../models/test_suite_models.dart';
import '../services/repo_root.dart';
import '../services/test_discovery.dart';
import '../services/test_inventory.dart';
import '../services/test_runner.dart';

class TestDashboardController extends ChangeNotifier {
  TestDashboardController({TestRunner? runner})
    : _runner = runner ?? TestRunner();

  final TestRunner _runner;

  String? _repoRoot;
  List<TestSuiteDefinition> _suites = const [];
  Map<String, DiscoveredTests> _discovered = const {};
  Map<String, SuiteRunResult> _runs = const {};
  String? _selectedSuiteId;
  String? _error;
  bool _loading = false;
  bool _runningAll = false;
  String? _runningSuiteId;
  DateTime? _lastFullRunAt;

  String? get repoRoot => _repoRoot;
  List<TestSuiteDefinition> get suites => _suites;
  Map<String, DiscoveredTests> get discovered => _discovered;
  Map<String, SuiteRunResult> get runs => _runs;
  String? get selectedSuiteId => _selectedSuiteId;
  String? get error => _error;
  bool get loading => _loading;
  bool get runningAll => _runningAll;
  String? get runningSuiteId => _runningSuiteId;

  TestSuiteDefinition? get selectedSuite {
    final id = _selectedSuiteId;
    if (id == null) return null;
    for (final suite in _suites) {
      if (suite.id == id) return suite;
    }
    return null;
  }

  SuiteRunResult? runFor(String suiteId) => _runs[suiteId];

  DashboardSnapshot get snapshot => DashboardSnapshot(
    repoRoot: _repoRoot ?? '',
    suites: _suites,
    discovered: _discovered,
    runs: _runs,
    focusItems: computeFocusItems(
      suites: _suites,
      discovered: _discovered,
      runs: _runs,
    ),
    lastFullRunAt: _lastFullRunAt,
  );

  Future<void> initialize() async {
    _loading = true;
    _error = null;
    notifyListeners();

    final root = findRepoRoot();
    if (root == null) {
      _error =
          'Could not find Retcon repository root. Set RETCON_ROOT or run from inside the repo.';
      _loading = false;
      notifyListeners();
      return;
    }

    _repoRoot = root;
    _suites = buildTestInventory(root);
    _discovered = discoverTestFiles(root, _suites);
    _selectedSuiteId ??= _suites.first.id;
    _loading = false;
    notifyListeners();
  }

  void selectSuite(String suiteId) {
    if (_selectedSuiteId == suiteId) return;
    _selectedSuiteId = suiteId;
    notifyListeners();
  }

  Future<void> runSuite(String suiteId) async {
    TestSuiteDefinition? suite;
    for (final item in _suites) {
      if (item.id == suiteId) {
        suite = item;
        break;
      }
    }
    if (suite == null) return;

    _runningSuiteId = suiteId;
    notifyListeners();

    final result = await _runner.runSuite(suite);
    _runs = {..._runs, suiteId: result};
    _runningSuiteId = null;
    notifyListeners();
  }

  Future<void> runAll({bool includeManual = false}) async {
    _runningAll = true;
    notifyListeners();

    for (final suite in _suites) {
      if (suite.manualOnly && !includeManual) continue;
      _runningSuiteId = suite.id;
      notifyListeners();
      final result = await _runner.runSuite(suite);
      _runs = {..._runs, suite.id: result};
    }

    _runningSuiteId = null;
    _runningAll = false;
    _lastFullRunAt = DateTime.now();
    notifyListeners();
  }

  Future<void> runFailed() async {
    final failedIds = _runs.entries
        .where((entry) => entry.value.status == SuiteRunStatus.failed)
        .map((entry) => entry.key)
        .toList();
    for (final id in failedIds) {
      await runSuite(id);
    }
  }

  void refreshDiscovery() {
    if (_repoRoot == null) return;
    _discovered = discoverTestFiles(_repoRoot!, _suites);
    notifyListeners();
  }
}

List<FocusItem> computeFocusItems({
  required List<TestSuiteDefinition> suites,
  required Map<String, DiscoveredTests> discovered,
  required Map<String, SuiteRunResult> runs,
}) {
  final items = <FocusItem>[];

  for (final suite in suites) {
    final run = runs[suite.id];
    final files = discovered[suite.id]?.fileCount ?? 0;

    if (run == null) {
      if (suite.manualOnly) {
        items.add(
          FocusItem(
            priority: FocusPriority.medium,
            category: FocusCategory.manualOnly,
            title: suite.name,
            detail: suite.manualReason ?? 'Manual validation required.',
            suiteId: suite.id,
            actionLabel: 'Run manually',
          ),
        );
      } else {
        items.add(
          FocusItem(
            priority: FocusPriority.low,
            category: FocusCategory.neverRun,
            title: suite.name,
            detail: 'Not run yet · $files test file(s) discovered',
            suiteId: suite.id,
            actionLabel: 'Run suite',
          ),
        );
      }
      continue;
    }

    for (final testCase in run.cases.where(
      (c) => c.status == TestCaseStatus.failed,
    )) {
      items.add(
        FocusItem(
          priority: FocusPriority.critical,
          category: FocusCategory.failure,
          title: testCase.name,
          detail: testCase.message ?? 'Failed in ${suite.name}',
          suiteId: suite.id,
          testName: testCase.name,
          actionLabel: 'View suite',
        ),
      );
    }

    if (run.status == SuiteRunStatus.failed && run.failedCount == 0) {
      items.add(
        FocusItem(
          priority: FocusPriority.critical,
          category: FocusCategory.failure,
          title: suite.name,
          detail: run.errorMessage ?? 'Suite exited with code ${run.exitCode}',
          suiteId: suite.id,
          actionLabel: 'View output',
        ),
      );
    }

    if (!suite.inCi && files > 0 && !suite.manualOnly) {
      items.add(
        FocusItem(
          priority: FocusPriority.high,
          category: FocusCategory.notInCi,
          title: suite.name,
          detail: '$files test file(s) not covered by GitHub Actions CI today',
          suiteId: suite.id,
          actionLabel: 'Run suite',
        ),
      );
    }

    if (suite.manualOnly && run.status == SuiteRunStatus.skipped) {
      items.add(
        FocusItem(
          priority: FocusPriority.medium,
          category: FocusCategory.manualOnly,
          title: suite.name,
          detail: suite.manualReason ?? 'Requires manual execution',
          suiteId: suite.id,
        ),
      );
    }
  }

  items.sort((a, b) {
    final byPriority = a.sortOrder.compareTo(b.sortOrder);
    if (byPriority != 0) return byPriority;
    return a.title.compareTo(b.title);
  });
  return items;
}
