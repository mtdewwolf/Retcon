import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

import '../controllers/test_dashboard_controller.dart';
import '../models/test_suite_models.dart';

class TestDashboardPage extends StatelessWidget {
  const TestDashboardPage({required this.controller, super.key});

  final TestDashboardController controller;

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: controller,
    builder: (context, _) {
      if (controller.loading) {
        return const Center(child: CircularProgressIndicator());
      }
      if (controller.error case final error?) {
        return _ErrorState(message: error, onRetry: controller.initialize);
      }
      return _DashboardBody(controller: controller);
    },
  );
}

class _ErrorState extends StatelessWidget {
  const _ErrorState({required this.message, required this.onRetry});

  final String message;
  final VoidCallback onRetry;

  @override
  Widget build(BuildContext context) => Center(
    child: RetconPanel(
      label: 'Repository not found',
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 520),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(message),
            const SizedBox(height: RetconSpacing.md),
            RetconButton(label: 'Retry', onPressed: onRetry),
          ],
        ),
      ),
    ),
  );
}

class _DashboardBody extends StatefulWidget {
  const _DashboardBody({required this.controller});

  final TestDashboardController controller;

  @override
  State<_DashboardBody> createState() => _DashboardBodyState();
}

class _DashboardBodyState extends State<_DashboardBody> {
  TestStack? _stackFilter;
  FocusCategory? _focusFilter;
  String _search = '';

  TestDashboardController get controller => widget.controller;

  @override
  Widget build(BuildContext context) {
    final snapshot = controller.snapshot;
    final filteredSuites = controller.suites.where((suite) {
      if (_stackFilter != null && suite.stack != _stackFilter) return false;
      if (_search.isEmpty) return true;
      final haystack = '${suite.name} ${suite.description ?? ''}'.toLowerCase();
      return haystack.contains(_search.toLowerCase());
    }).toList();

    final focusItems = _focusFilter == null
        ? snapshot.focusItems
        : snapshot.focusItems
              .where((item) => item.category == _focusFilter)
              .toList();

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _SummaryHeader(
          snapshot: snapshot,
          runningAll: controller.runningAll,
          runningSuiteId: controller.runningSuiteId,
          onRunAll: () => controller.runAll(),
          onRunFailed:
              controller.runs.values.any(
                (run) => run.status == SuiteRunStatus.failed,
              )
              ? controller.runFailed
              : null,
          onRefreshDiscovery: controller.refreshDiscovery,
        ),
        Expanded(
          child: RetconSplitter(
            initialRatio: 0.22,
            minFirst: 220,
            minSecond: 480,
            semanticsLabel: 'Suite list and detail panes',
            first: _SuiteSidebar(
              suites: filteredSuites,
              runs: controller.runs,
              discovered: controller.discovered,
              selectedId: controller.selectedSuiteId,
              runningSuiteId: controller.runningSuiteId,
              stackFilter: _stackFilter,
              search: _search,
              onStackFilterChanged: (value) =>
                  setState(() => _stackFilter = value),
              onSearchChanged: (value) => setState(() => _search = value),
              onSelect: controller.selectSuite,
              onRun: controller.runSuite,
            ),
            second: RetconSplitter(
              initialRatio: 0.62,
              minFirst: 320,
              minSecond: 260,
              semanticsLabel: 'Suite detail and focus panes',
              first: _SuiteDetailPanel(
                suite: controller.selectedSuite,
                run: controller.selectedSuiteId == null
                    ? null
                    : controller.runFor(controller.selectedSuiteId!),
                discovered: controller.selectedSuiteId == null
                    ? null
                    : controller.discovered[controller.selectedSuiteId!],
                running:
                    controller.runningSuiteId == controller.selectedSuiteId,
                onRun: controller.selectedSuiteId == null
                    ? null
                    : () => controller.runSuite(controller.selectedSuiteId!),
              ),
              second: _FocusPanel(
                items: focusItems,
                categoryFilter: _focusFilter,
                onCategoryFilterChanged: (value) =>
                    setState(() => _focusFilter = value),
                onSelectSuite: controller.selectSuite,
              ),
            ),
          ),
        ),
      ],
    );
  }
}

class _SummaryHeader extends StatelessWidget {
  const _SummaryHeader({
    required this.snapshot,
    required this.runningAll,
    required this.runningSuiteId,
    required this.onRunAll,
    required this.onRefreshDiscovery,
    this.onRunFailed,
  });

  final DashboardSnapshot snapshot;
  final bool runningAll;
  final String? runningSuiteId;
  final VoidCallback onRunAll;
  final VoidCallback? onRunFailed;
  final VoidCallback onRefreshDiscovery;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context).textTheme;
    final busy = runningAll || runningSuiteId != null;

    return RetconPanel(
      padding: const EdgeInsets.symmetric(
        horizontal: RetconSpacing.md,
        vertical: RetconSpacing.sm,
      ),
      child: Row(
        children: [
          Icon(Icons.science, color: Theme.of(context).colorScheme.primary),
          const SizedBox(width: RetconSpacing.sm),
          Text('Test Suite Dashboard', style: theme.titleMedium),
          const SizedBox(width: RetconSpacing.md),
          RetconBadge(
            label: '${snapshot.totalSuites} suites',
            status: RetconStatus.neutral,
          ),
          const SizedBox(width: RetconSpacing.xs),
          RetconBadge(
            label: '${snapshot.totalTestFiles} files',
            status: RetconStatus.neutral,
          ),
          const SizedBox(width: RetconSpacing.xs),
          RetconBadge(
            label: '${snapshot.suitesPassed} passed',
            status: snapshot.suitesFailed > 0
                ? RetconStatus.warning
                : RetconStatus.success,
          ),
          if (snapshot.totalFailedCases > 0) ...[
            const SizedBox(width: RetconSpacing.xs),
            RetconBadge(
              label: '${snapshot.totalFailedCases} failing cases',
              status: RetconStatus.error,
            ),
          ],
          const SizedBox(width: RetconSpacing.xs),
          RetconBadge(
            label: '${snapshot.focusItems.length} focus items',
            status:
                snapshot.focusItems.any(
                  (item) => item.priority == FocusPriority.critical,
                )
                ? RetconStatus.error
                : RetconStatus.warning,
          ),
          const Spacer(),
          if (busy)
            const Padding(
              padding: EdgeInsets.only(right: RetconSpacing.sm),
              child: SizedBox(
                width: 18,
                height: 18,
                child: CircularProgressIndicator(strokeWidth: 2),
              ),
            ),
          RetconButton(
            label: 'Rescan files',
            onPressed: busy ? null : onRefreshDiscovery,
          ),
          const SizedBox(width: RetconSpacing.xs),
          if (onRunFailed != null)
            RetconButton(
              label: 'Rerun failed',
              onPressed: busy ? null : onRunFailed,
            ),
          const SizedBox(width: RetconSpacing.xs),
          RetconButton(
            label: runningAll ? 'Running…' : 'Run all',
            onPressed: busy ? null : onRunAll,
          ),
        ],
      ),
    );
  }
}

class _SuiteSidebar extends StatelessWidget {
  const _SuiteSidebar({
    required this.suites,
    required this.runs,
    required this.discovered,
    required this.selectedId,
    required this.runningSuiteId,
    required this.stackFilter,
    required this.search,
    required this.onStackFilterChanged,
    required this.onSearchChanged,
    required this.onSelect,
    required this.onRun,
  });

  final List<TestSuiteDefinition> suites;
  final Map<String, SuiteRunResult> runs;
  final Map<String, DiscoveredTests> discovered;
  final String? selectedId;
  final String? runningSuiteId;
  final TestStack? stackFilter;
  final String search;
  final ValueChanged<TestStack?> onStackFilterChanged;
  final ValueChanged<String> onSearchChanged;
  final ValueChanged<String> onSelect;
  final ValueChanged<String> onRun;

  @override
  Widget build(BuildContext context) => RetconPanel(
    label: 'Suites',
    recessed: true,
    padding: const EdgeInsets.all(RetconSpacing.sm),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        TextField(
          decoration: const InputDecoration(
            hintText: 'Search suites…',
            prefixIcon: Icon(Icons.search),
            isDense: true,
          ),
          onChanged: onSearchChanged,
        ),
        const SizedBox(height: RetconSpacing.sm),
        Wrap(
          spacing: RetconSpacing.xs,
          runSpacing: RetconSpacing.xs,
          children: [
            FilterChip(
              label: const Text('All'),
              selected: stackFilter == null,
              onSelected: (_) => onStackFilterChanged(null),
            ),
            for (final stack in TestStack.values)
              FilterChip(
                label: Text(_stackLabel(stack)),
                selected: stackFilter == stack,
                onSelected: (_) => onStackFilterChanged(stack),
              ),
          ],
        ),
        const SizedBox(height: RetconSpacing.sm),
        Expanded(
          child: ListView.separated(
            itemCount: suites.length,
            separatorBuilder: (_, _) =>
                const SizedBox(height: RetconSpacing.xs),
            itemBuilder: (context, index) {
              final suite = suites[index];
              final run = runs[suite.id];
              final selected = suite.id == selectedId;
              final running = suite.id == runningSuiteId;
              return _SuiteTile(
                suite: suite,
                run: run,
                fileCount: discovered[suite.id]?.fileCount ?? 0,
                selected: selected,
                running: running,
                onTap: () => onSelect(suite.id),
                onRun: () => onRun(suite.id),
              );
            },
          ),
        ),
      ],
    ),
  );
}

class _SuiteTile extends StatelessWidget {
  const _SuiteTile({
    required this.suite,
    required this.run,
    required this.fileCount,
    required this.selected,
    required this.running,
    required this.onTap,
    required this.onRun,
  });

  final TestSuiteDefinition suite;
  final SuiteRunResult? run;
  final int fileCount;
  final bool selected;
  final bool running;
  final VoidCallback onTap;
  final VoidCallback onRun;

  @override
  Widget build(BuildContext context) {
    final (badgeLabel, badgeStatus) = _statusBadge(run, suite);
    return Material(
      color: selected
          ? Theme.of(
              context,
            ).colorScheme.primaryContainer.withValues(alpha: 0.35)
          : Colors.transparent,
      borderRadius: BorderRadius.circular(RetconSpacing.xs),
      child: InkWell(
        onTap: onTap,
        borderRadius: BorderRadius.circular(RetconSpacing.xs),
        child: Padding(
          padding: const EdgeInsets.all(RetconSpacing.sm),
          child: Row(
            children: [
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      suite.name,
                      style: Theme.of(context).textTheme.titleSmall,
                    ),
                    const SizedBox(height: 2),
                    Text(
                      '${_stackLabel(suite.stack)} · $fileCount files'
                      '${suite.inCi ? '' : ' · not in CI'}',
                      style: Theme.of(context).textTheme.bodySmall,
                    ),
                  ],
                ),
              ),
              if (running)
                const SizedBox(
                  width: 16,
                  height: 16,
                  child: CircularProgressIndicator(strokeWidth: 2),
                )
              else
                RetconBadge(label: badgeLabel, status: badgeStatus),
              IconButton(
                tooltip: 'Run ${suite.name}',
                icon: const Icon(Icons.play_arrow),
                onPressed: running ? null : onRun,
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _SuiteDetailPanel extends StatelessWidget {
  const _SuiteDetailPanel({
    required this.suite,
    required this.run,
    required this.discovered,
    required this.running,
    this.onRun,
  });

  final TestSuiteDefinition? suite;
  final SuiteRunResult? run;
  final DiscoveredTests? discovered;
  final bool running;
  final VoidCallback? onRun;

  @override
  Widget build(BuildContext context) {
    if (suite == null) {
      return const Center(child: Text('Select a suite'));
    }

    final cases = run?.cases ?? const [];
    final failedCases = cases.where((c) => c.status == TestCaseStatus.failed);
    final passedCases = cases.where((c) => c.status == TestCaseStatus.passed);

    return RetconPanel(
      label: suite!.name,
      padding: const EdgeInsets.all(RetconSpacing.sm),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    if (suite!.description case final description?)
                      Text(description),
                    const SizedBox(height: RetconSpacing.xs),
                    Text(
                      '${suite!.command} ${suite!.commandArgs.join(' ')}',
                      style: Theme.of(
                        context,
                      ).textTheme.bodySmall?.copyWith(fontFamily: 'monospace'),
                    ),
                    if (suite!.ciJob case final job?)
                      Text(
                        'CI: $job',
                        style: Theme.of(context).textTheme.bodySmall,
                      ),
                    if (suite!.manualReason case final reason?)
                      Text(
                        reason,
                        style: TextStyle(
                          color: Theme.of(context).colorScheme.tertiary,
                        ),
                      ),
                  ],
                ),
              ),
              if (running)
                const CircularProgressIndicator()
              else
                RetconButton(label: 'Run suite', onPressed: onRun),
              if (run?.stdout case final output? when output.isNotEmpty) ...[
                const SizedBox(width: RetconSpacing.xs),
                IconButton(
                  tooltip: 'Copy output',
                  icon: const Icon(Icons.copy),
                  onPressed: () =>
                      Clipboard.setData(ClipboardData(text: output)),
                ),
              ],
            ],
          ),
          if (run case final result?) ...[
            const SizedBox(height: RetconSpacing.sm),
            Wrap(
              spacing: RetconSpacing.xs,
              runSpacing: RetconSpacing.xs,
              children: [
                RetconBadge(
                  label: result.status.name,
                  status: switch (result.status) {
                    SuiteRunStatus.passed => RetconStatus.success,
                    SuiteRunStatus.failed ||
                    SuiteRunStatus.error => RetconStatus.error,
                    SuiteRunStatus.running => RetconStatus.warning,
                    SuiteRunStatus.skipped ||
                    SuiteRunStatus.idle => RetconStatus.neutral,
                  },
                ),
                RetconBadge(
                  label: '${result.passedCount} passed',
                  status: RetconStatus.success,
                ),
                if (result.failedCount > 0)
                  RetconBadge(
                    label: '${result.failedCount} failed',
                    status: RetconStatus.error,
                  ),
                if (result.duration case final duration?)
                  RetconBadge(
                    label: '${duration.inSeconds}s',
                    status: RetconStatus.neutral,
                  ),
              ],
            ),
          ],
          const SizedBox(height: RetconSpacing.sm),
          Expanded(
            child: DefaultTabController(
              length: 3,
              child: Column(
                children: [
                  TabBar(
                    tabs: [
                      Tab(text: 'All (${cases.length})'),
                      Tab(text: 'Failed (${failedCases.length})'),
                      Tab(text: 'Passed (${passedCases.length})'),
                    ],
                  ),
                  Expanded(
                    child: TabBarView(
                      children: [
                        _CaseList(cases: cases),
                        _CaseList(cases: failedCases.toList()),
                        _CaseList(cases: passedCases.toList()),
                      ],
                    ),
                  ),
                ],
              ),
            ),
          ),
          if (discovered?.files.isNotEmpty ?? false) ...[
            const Divider(),
            Text(
              'Discovered files (${discovered!.fileCount})',
              style: Theme.of(context).textTheme.titleSmall,
            ),
            const SizedBox(height: RetconSpacing.xs),
            SizedBox(
              height: 72,
              child: ListView.builder(
                itemCount: discovered!.files.length,
                itemBuilder: (context, index) => Text(
                  discovered!.files[index],
                  style: Theme.of(
                    context,
                  ).textTheme.bodySmall?.copyWith(fontFamily: 'monospace'),
                ),
              ),
            ),
          ],
        ],
      ),
    );
  }
}

class _CaseList extends StatelessWidget {
  const _CaseList({required this.cases});

  final List<TestCaseResult> cases;

  @override
  Widget build(BuildContext context) {
    if (cases.isEmpty) {
      return const Center(child: Text('No test cases in this view'));
    }
    return RetconTable(
      semanticsLabel: 'Test cases',
      columns: const [
        RetconTableColumn(label: 'Status'),
        RetconTableColumn(label: 'Test'),
        RetconTableColumn(label: 'Details'),
      ],
      rows: [
        for (final testCase in cases)
          RetconTableRow(
            cells: [
              _caseStatusLabel(testCase.status),
              testCase.name,
              testCase.message ?? '',
            ],
            semanticsLabel: testCase.name,
          ),
      ],
    );
  }
}

class _FocusPanel extends StatelessWidget {
  const _FocusPanel({
    required this.items,
    required this.categoryFilter,
    required this.onCategoryFilterChanged,
    required this.onSelectSuite,
  });

  final List<FocusItem> items;
  final FocusCategory? categoryFilter;
  final ValueChanged<FocusCategory?> onCategoryFilterChanged;
  final ValueChanged<String> onSelectSuite;

  @override
  Widget build(BuildContext context) => RetconPanel(
    label: 'Focus',
    recessed: true,
    padding: const EdgeInsets.all(RetconSpacing.sm),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          'What needs attention',
          style: Theme.of(context).textTheme.titleSmall,
        ),
        const SizedBox(height: RetconSpacing.xs),
        Wrap(
          spacing: RetconSpacing.xs,
          runSpacing: RetconSpacing.xs,
          children: [
            FilterChip(
              label: const Text('All'),
              selected: categoryFilter == null,
              onSelected: (_) => onCategoryFilterChanged(null),
            ),
            for (final category in FocusCategory.values)
              FilterChip(
                label: Text(_focusCategoryLabel(category)),
                selected: categoryFilter == category,
                onSelected: (_) => onCategoryFilterChanged(category),
              ),
          ],
        ),
        const SizedBox(height: RetconSpacing.sm),
        Expanded(
          child: items.isEmpty
              ? const Center(
                  child: Text('Nothing flagged — run suites to populate'),
                )
              : ListView.separated(
                  itemCount: items.length,
                  separatorBuilder: (_, _) =>
                      const SizedBox(height: RetconSpacing.xs),
                  itemBuilder: (context, index) {
                    final item = items[index];
                    return _FocusTile(
                      item: item,
                      onTap: item.suiteId == null
                          ? null
                          : () => onSelectSuite(item.suiteId!),
                    );
                  },
                ),
        ),
      ],
    ),
  );
}

class _FocusTile extends StatelessWidget {
  const _FocusTile({required this.item, this.onTap});

  final FocusItem item;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final status = switch (item.priority) {
      FocusPriority.critical => RetconStatus.error,
      FocusPriority.high => RetconStatus.warning,
      FocusPriority.medium => RetconStatus.warning,
      FocusPriority.low => RetconStatus.neutral,
    };

    return Material(
      color: Theme.of(context).colorScheme.surfaceContainerHighest,
      borderRadius: BorderRadius.circular(RetconSpacing.xs),
      child: InkWell(
        onTap: onTap,
        borderRadius: BorderRadius.circular(RetconSpacing.xs),
        child: Padding(
          padding: const EdgeInsets.all(RetconSpacing.sm),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  RetconBadge(label: item.priority.name, status: status),
                  const SizedBox(width: RetconSpacing.xs),
                  Expanded(
                    child: Text(
                      item.title,
                      style: Theme.of(context).textTheme.titleSmall,
                    ),
                  ),
                ],
              ),
              const SizedBox(height: RetconSpacing.xs),
              Text(item.detail),
              if (item.actionLabel case final label?)
                Padding(
                  padding: const EdgeInsets.only(top: RetconSpacing.xs),
                  child: Text(
                    label,
                    style: TextStyle(
                      color: Theme.of(context).colorScheme.primary,
                    ),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

(String, RetconStatus) _statusBadge(
  SuiteRunResult? run,
  TestSuiteDefinition suite,
) {
  if (run == null) {
    return suite.manualOnly
        ? ('Manual', RetconStatus.warning)
        : ('Not run', RetconStatus.neutral);
  }
  return switch (run.status) {
    SuiteRunStatus.passed => ('Passed', RetconStatus.success),
    SuiteRunStatus.failed ||
    SuiteRunStatus.error => ('Failed', RetconStatus.error),
    SuiteRunStatus.running => ('Running', RetconStatus.warning),
    SuiteRunStatus.skipped => ('Skipped', RetconStatus.warning),
    SuiteRunStatus.idle => ('Idle', RetconStatus.neutral),
  };
}

String _stackLabel(TestStack stack) => switch (stack) {
  TestStack.rust => 'Rust',
  TestStack.flutter => 'Flutter',
  TestStack.bun => 'Bun',
  TestStack.node => 'Node',
  TestStack.protocol => 'Protocol',
  TestStack.powershell => 'PowerShell',
};

String _caseStatusLabel(TestCaseStatus status) => switch (status) {
  TestCaseStatus.passed => 'PASS',
  TestCaseStatus.failed => 'FAIL',
  TestCaseStatus.skipped => 'SKIP',
  TestCaseStatus.ignored => 'IGNORE',
};

String _focusCategoryLabel(FocusCategory category) => switch (category) {
  FocusCategory.failure => 'Failures',
  FocusCategory.notInCi => 'Not in CI',
  FocusCategory.manualOnly => 'Manual',
  FocusCategory.neverRun => 'Never run',
  FocusCategory.stale => 'Stale',
  FocusCategory.coverageGap => 'Coverage gap',
};
