import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

import '../tasks/task_models.dart';
import 'verification_controller.dart';
import 'verification_models.dart';

class TaskVerificationSection extends StatelessWidget {
  const TaskVerificationSection({
    required this.controller,
    required this.task,
    super.key,
    this.onOpenPreview,
    this.previewReady = false,
  });

  final VerificationController controller;
  final RoadmapTask task;
  final Future<void> Function()? onOpenPreview;
  final bool previewReady;

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: controller,
    builder: (context, _) {
      final blocker = controller.completionBlocker;
      return RetconPanel(
        key: const Key('task-verification-section'),
        label: 'Verification gates',
        recessed: true,
        padding: const EdgeInsets.all(RetconSpacing.sm),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Row(
              children: [
                Expanded(
                  child: Text(
                    'Verification gates',
                    style: Theme.of(context).textTheme.titleMedium,
                  ),
                ),
                RetconBadge(
                  label: controller.loading
                      ? 'Loading'
                      : controller.allowsCompletion
                      ? 'Required gates passed'
                      : '${controller.gates.where((gate) => gate.enabled && gate.required).length} required',
                  status: controller.loading
                      ? RetconStatus.warning
                      : controller.allowsCompletion
                      ? RetconStatus.success
                      : RetconStatus.error,
                ),
                if (onOpenPreview != null)
                  IconButton(
                    key: const Key('verification-open-preview'),
                    tooltip: previewReady
                        ? 'Open preview'
                        : 'Start and preview',
                    onPressed: onOpenPreview,
                    icon: Icon(
                      previewReady
                          ? Icons.open_in_browser
                          : Icons.rocket_launch,
                    ),
                  ),
              ],
            ),
            if (blocker != null) ...[
              const SizedBox(height: RetconSpacing.xs),
              Text(
                blocker,
                key: const Key('verification-completion-blocker'),
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            ],
            if (controller.latestRun case final run?) ...[
              const SizedBox(height: RetconSpacing.xs),
              Text(
                'Last run: ${_runLabel(run.status)} · ${_duration(run.duration)} · '
                '${run.passedTests} passed, ${run.failedTests} failed',
              ),
            ],
            const SizedBox(height: RetconSpacing.sm),
            Wrap(
              spacing: RetconSpacing.xs,
              runSpacing: RetconSpacing.xs,
              children: [
                FilledButton.icon(
                  key: const Key('run-verification'),
                  onPressed: controller.running || controller.gates.isEmpty
                      ? null
                      : controller.runAll,
                  icon: const Icon(Icons.play_arrow),
                  label: const Text('Run verification'),
                ),
                OutlinedButton.icon(
                  key: const Key('open-verification-center'),
                  onPressed: () => VerificationDialog.show(
                    context,
                    controller: controller,
                    task: task,
                  ),
                  icon: const Icon(Icons.fact_check_outlined),
                  label: const Text('Open verification center'),
                ),
              ],
            ),
          ],
        ),
      );
    },
  );
}

class VerificationDialog extends StatelessWidget {
  const VerificationDialog({
    required this.controller,
    required this.task,
    super.key,
  });

  final VerificationController controller;
  final RoadmapTask task;

  static Future<void> show(
    BuildContext context, {
    required VerificationController controller,
    required RoadmapTask task,
  }) => showDialog<void>(
    context: context,
    builder: (context) =>
        VerificationDialog(controller: controller, task: task),
  );

  @override
  Widget build(BuildContext context) => Dialog(
    insetPadding: const EdgeInsets.all(RetconSpacing.md),
    child: SizedBox(
      width: 1080,
      height: 720,
      child: Column(
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(
              RetconSpacing.md,
              RetconSpacing.sm,
              RetconSpacing.xs,
              0,
            ),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    'Verification · ${task.title}',
                    style: Theme.of(context).textTheme.titleLarge,
                  ),
                ),
                IconButton(
                  tooltip: 'Close verification center',
                  onPressed: () => Navigator.of(context).pop(),
                  icon: const Icon(Icons.close),
                ),
              ],
            ),
          ),
          Expanded(
            child: VerificationPanel(controller: controller, task: task),
          ),
        ],
      ),
    ),
  );
}

class VerificationPanel extends StatefulWidget {
  const VerificationPanel({
    required this.controller,
    required this.task,
    super.key,
    this.onOpenFile,
  });

  final VerificationController controller;
  final RoadmapTask task;
  final ValueChanged<VerificationFileLink>? onOpenFile;

  @override
  State<VerificationPanel> createState() => _VerificationPanelState();
}

class _VerificationPanelState extends State<VerificationPanel> {
  bool _showStderr = false;

  @override
  Widget build(BuildContext context) => DefaultTabController(
    length: 4,
    child: AnimatedBuilder(
      animation: widget.controller,
      builder: (context, _) {
        final controller = widget.controller;
        return Column(
          children: [
            _RunToolbar(controller: controller),
            const TabBar(
              tabs: [
                Tab(icon: Icon(Icons.tune), text: 'Setup'),
                Tab(icon: Icon(Icons.timeline), text: 'Live run'),
                Tab(icon: Icon(Icons.history), text: 'History'),
                Tab(icon: Icon(Icons.summarize), text: 'Completion report'),
              ],
            ),
            if (controller.error != null)
              MaterialBanner(
                content: Text(controller.error!),
                actions: [
                  TextButton(
                    onPressed: controller.load,
                    child: const Text('Retry'),
                  ),
                ],
              ),
            Expanded(
              child: controller.loading && !controller.loaded
                  ? const Center(child: CircularProgressIndicator())
                  : TabBarView(
                      children: [
                        _SetupTab(controller: controller),
                        _LiveRunTab(
                          controller: controller,
                          showStderr: _showStderr,
                          onOutputChanged: (value) =>
                              setState(() => _showStderr = value),
                          onOpenFile: widget.onOpenFile,
                        ),
                        _HistoryTab(controller: controller),
                        CompletionReportPanel(
                          task: widget.task,
                          controller: controller,
                        ),
                      ],
                    ),
            ),
          ],
        );
      },
    ),
  );
}

class _RunToolbar extends StatelessWidget {
  const _RunToolbar({required this.controller});
  final VerificationController controller;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.symmetric(horizontal: RetconSpacing.md),
    child: Row(
      children: [
        RetconBadge(
          label: controller.running
              ? 'Running'
              : controller.allowsCompletion
              ? 'Completion ready'
              : 'Verification required',
          status: controller.running
              ? RetconStatus.warning
              : controller.allowsCompletion
              ? RetconStatus.success
              : RetconStatus.error,
        ),
        const Spacer(),
        TextButton.icon(
          key: const Key('verification-run-all'),
          onPressed: controller.running || controller.gates.isEmpty
              ? null
              : controller.runAll,
          icon: const Icon(Icons.play_arrow),
          label: const Text('Run all'),
        ),
        TextButton.icon(
          key: const Key('verification-rerun-failed'),
          onPressed:
              controller.running ||
                  !(controller.latestRun?.gates.any(
                        (gate) => gate.status == GateStatus.failed,
                      ) ??
                      false)
              ? null
              : controller.rerunFailed,
          icon: const Icon(Icons.replay),
          label: const Text('Rerun failed'),
        ),
        TextButton.icon(
          key: const Key('verification-cancel'),
          onPressed: controller.running ? controller.cancel : null,
          icon: const Icon(Icons.stop),
          label: const Text('Cancel'),
        ),
      ],
    ),
  );
}

class _SetupTab extends StatelessWidget {
  const _SetupTab({required this.controller});
  final VerificationController controller;

  @override
  Widget build(BuildContext context) => ListView(
    padding: const EdgeInsets.all(RetconSpacing.md),
    children: [
      Row(
        children: [
          Expanded(
            child: Text(
              'Project commands',
              style: Theme.of(context).textTheme.titleMedium,
            ),
          ),
          TextButton.icon(
            key: const Key('detect-project-commands'),
            onPressed: controller.detectCommands,
            icon: const Icon(Icons.manage_search),
            label: const Text('Detect commands'),
          ),
        ],
      ),
      if (controller.commands.isEmpty)
        const Text(
          'No commands detected. Add overrides after detection is available.',
        ),
      for (final command in controller.commands)
        Card(
          child: ListTile(
            title: Text(command.label),
            subtitle: SelectableText(command.command),
            leading: Icon(
              command.source == CommandSource.override
                  ? Icons.edit
                  : Icons.auto_awesome,
            ),
            trailing: Wrap(
              children: [
                IconButton(
                  tooltip: 'Edit command override',
                  onPressed: () => _editCommand(context, command),
                  icon: const Icon(Icons.edit_outlined),
                ),
                IconButton(
                  tooltip: 'Add as verification gate',
                  onPressed: () => controller.addGate(command),
                  icon: const Icon(Icons.add_task),
                ),
              ],
            ),
          ),
        ),
      const Divider(height: RetconSpacing.lg),
      Text(
        'Ordered verification gates',
        style: Theme.of(context).textTheme.titleMedium,
      ),
      const SizedBox(height: RetconSpacing.xs),
      if (controller.gates.isEmpty)
        const Text('Add a detected command to configure verification.'),
      for (var index = 0; index < controller.gates.length; index++)
        _GateEditorRow(
          gate: controller.gates[index],
          index: index,
          count: controller.gates.length,
          controller: controller,
        ),
    ],
  );

  Future<void> _editCommand(
    BuildContext context,
    ProjectCommand command,
  ) async {
    final value = await _prompt(
      context,
      title: 'Command override',
      label: command.label,
      initialValue: command.command,
    );
    if (value != null && value.trim().isNotEmpty) {
      await controller.overrideCommand(command, value);
    }
  }
}

class _GateEditorRow extends StatelessWidget {
  const _GateEditorRow({
    required this.gate,
    required this.index,
    required this.count,
    required this.controller,
  });
  final VerificationGate gate;
  final int index;
  final int count;
  final VerificationController controller;

  @override
  Widget build(BuildContext context) => Card(
    key: Key('verification-gate-${gate.id}'),
    child: Padding(
      padding: const EdgeInsets.all(RetconSpacing.xs),
      child: Row(
        children: [
          Switch(
            value: gate.enabled,
            onChanged: (value) =>
                controller.updateGate(gate.copyWith(enabled: value)),
          ),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(gate.label),
                Text(
                  gate.command,
                  style: Theme.of(context).textTheme.bodySmall,
                ),
              ],
            ),
          ),
          FilterChip(
            label: Text(gate.required ? 'Required' : 'Optional'),
            selected: gate.required,
            onSelected: (value) =>
                controller.updateGate(gate.copyWith(required: value)),
          ),
          IconButton(
            tooltip: 'Move gate up',
            onPressed: index == 0
                ? null
                : () => controller.moveGate(gate.id, -1),
            icon: const Icon(Icons.arrow_upward),
          ),
          IconButton(
            tooltip: 'Move gate down',
            onPressed: index == count - 1
                ? null
                : () => controller.moveGate(gate.id, 1),
            icon: const Icon(Icons.arrow_downward),
          ),
          IconButton(
            tooltip: 'Edit gate command',
            onPressed: () => _edit(context),
            icon: const Icon(Icons.edit_outlined),
          ),
          IconButton(
            tooltip: 'Remove gate',
            onPressed: () => controller.removeGate(gate.id),
            icon: const Icon(Icons.delete_outline),
          ),
        ],
      ),
    ),
  );

  Future<void> _edit(BuildContext context) async {
    final value = await _prompt(
      context,
      title: 'Edit gate command',
      label: gate.label,
      initialValue: gate.command,
    );
    if (value != null && value.trim().isNotEmpty) {
      await controller.updateGate(gate.copyWith(command: value.trim()));
    }
  }
}

class _LiveRunTab extends StatelessWidget {
  const _LiveRunTab({
    required this.controller,
    required this.showStderr,
    required this.onOutputChanged,
    this.onOpenFile,
  });
  final VerificationController controller;
  final bool showStderr;
  final ValueChanged<bool> onOutputChanged;
  final ValueChanged<VerificationFileLink>? onOpenFile;

  @override
  Widget build(BuildContext context) {
    final run = controller.activeRun ?? controller.latestRun;
    if (run == null) {
      return const Center(
        child: Text('Run verification to see a live gate timeline.'),
      );
    }
    final output = controller.selectedOutput;
    return ListView(
      padding: const EdgeInsets.all(RetconSpacing.md),
      children: [
        Text('Gate timeline', style: Theme.of(context).textTheme.titleMedium),
        const SizedBox(height: RetconSpacing.xs),
        for (final gate in run.gates)
          _GateTimelineRow(
            execution: gate,
            selected: controller.selectedOutputGateId == gate.gateId,
            onSelected: () => controller.selectOutput(gate.gateId),
            onOpenFile: onOpenFile,
          ),
        const Divider(height: RetconSpacing.lg),
        Row(
          children: [
            Expanded(
              child: Text(
                'Bounded raw output',
                style: Theme.of(context).textTheme.titleMedium,
              ),
            ),
            SegmentedButton<bool>(
              segments: const [
                ButtonSegment(value: false, label: Text('stdout')),
                ButtonSegment(value: true, label: Text('stderr')),
              ],
              selected: {showStderr},
              onSelectionChanged: (value) => onOutputChanged(value.single),
            ),
          ],
        ),
        const SizedBox(height: RetconSpacing.xs),
        Container(
          key: const Key('verification-raw-output'),
          constraints: const BoxConstraints(minHeight: 140, maxHeight: 220),
          padding: const EdgeInsets.all(RetconSpacing.sm),
          color: Theme.of(context).colorScheme.surfaceContainerLowest,
          child: SingleChildScrollView(
            child: SelectableText(
              showStderr ? output?.stderr ?? '' : output?.stdout ?? '',
              style: const TextStyle(fontFamily: 'monospace'),
            ),
          ),
        ),
      ],
    );
  }
}

class _GateTimelineRow extends StatelessWidget {
  const _GateTimelineRow({
    required this.execution,
    required this.selected,
    required this.onSelected,
    this.onOpenFile,
  });
  final GateExecution execution;
  final bool selected;
  final VoidCallback onSelected;
  final ValueChanged<VerificationFileLink>? onOpenFile;

  @override
  Widget build(BuildContext context) => Card(
    color: selected ? Theme.of(context).colorScheme.primaryContainer : null,
    child: InkWell(
      onTap: onSelected,
      child: Padding(
        padding: const EdgeInsets.all(RetconSpacing.sm),
        child: Row(
          children: [
            Icon(
              _gateIcon(execution.status),
              color: _gateColor(context, execution.status),
            ),
            const SizedBox(width: RetconSpacing.sm),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(execution.label),
                  Text(
                    '${execution.required ? 'Required' : 'Optional'} · '
                    '${execution.duration == null ? '—' : _duration(execution.duration!)} · '
                    '${execution.tests.passed} passed, ${execution.tests.failed} failed, '
                    '${execution.tests.skipped} skipped',
                    style: Theme.of(context).textTheme.bodySmall,
                  ),
                  if (execution.fileLinks.isNotEmpty)
                    Wrap(
                      spacing: RetconSpacing.xs,
                      children: [
                        for (final link in execution.fileLinks)
                          TextButton.icon(
                            onPressed: () => _openFile(link),
                            icon: const Icon(Icons.insert_drive_file_outlined),
                            label: Text(
                              '${link.path}${link.line == null ? '' : ':${link.line}'}',
                            ),
                          ),
                      ],
                    ),
                ],
              ),
            ),
            RetconBadge(
              label: _gateLabel(execution.status),
              status: _gateRetconStatus(execution.status),
            ),
          ],
        ),
      ),
    ),
  );

  void _openFile(VerificationFileLink link) {
    if (onOpenFile != null) {
      onOpenFile!(link);
    } else {
      Clipboard.setData(ClipboardData(text: '${link.path}:${link.line ?? 1}'));
    }
  }
}

class _HistoryTab extends StatelessWidget {
  const _HistoryTab({required this.controller});
  final VerificationController controller;

  @override
  Widget build(BuildContext context) {
    final comparison = controller.comparison;
    return ListView(
      padding: const EdgeInsets.all(RetconSpacing.md),
      children: [
        Text(
          'Persisted verification history',
          style: Theme.of(context).textTheme.titleMedium,
        ),
        if (comparison != null) ...[
          const SizedBox(height: RetconSpacing.sm),
          RetconPanel(
            label: 'Latest versus previous',
            recessed: true,
            child: Wrap(
              spacing: RetconSpacing.lg,
              runSpacing: RetconSpacing.sm,
              children: [
                Text(
                  'Latest versus previous',
                  style: Theme.of(context).textTheme.titleSmall,
                ),
                _Metric(
                  label: 'Passed tests Δ',
                  value: _signed(comparison.passedTestDelta),
                ),
                _Metric(
                  label: 'Failed tests Δ',
                  value: _signed(comparison.failedTestDelta),
                ),
                _Metric(
                  label: 'Duration Δ',
                  value:
                      '${comparison.durationDelta.isNegative ? '' : '+'}'
                      '${comparison.durationDelta.inMilliseconds} ms',
                ),
              ],
            ),
          ),
        ],
        const SizedBox(height: RetconSpacing.sm),
        if (controller.history.isEmpty)
          const Text('No persisted verification runs yet.'),
        for (final run in controller.history)
          Card(
            child: ListTile(
              leading: Icon(_runIcon(run.status)),
              title: Text(
                '${_runLabel(run.status)} · ${_duration(run.duration)}',
              ),
              subtitle: Text(
                '${run.startedAt.toLocal()} · ${run.passedTests} passed · '
                '${run.failedTests} failed · ${run.gates.length} gates',
              ),
            ),
          ),
      ],
    );
  }
}

class CompletionReportPanel extends StatelessWidget {
  const CompletionReportPanel({
    required this.task,
    required this.controller,
    super.key,
  });
  final RoadmapTask task;
  final VerificationController controller;

  @override
  Widget build(BuildContext context) {
    final run = controller.latestRun;
    final report = controller.report;
    final completeSteps = task.steps
        .where((step) => step.status == PlanStepStatus.complete)
        .length;
    final passedCriteria = task.criteria
        .where((criterion) => criterion.status == CriterionStatus.passed)
        .length;
    return ListView(
      key: const Key('completion-report-panel'),
      padding: const EdgeInsets.all(RetconSpacing.md),
      children: [
        Text(
          'Roadmap completion report',
          style: Theme.of(context).textTheme.titleLarge,
        ),
        const SizedBox(height: RetconSpacing.sm),
        RetconTable(
          columns: const [
            RetconTableColumn(label: 'Field'),
            RetconTableColumn(label: 'Value'),
          ],
          rows: [
            RetconTableRow(cells: ['Task', task.title]),
            RetconTableRow(cells: ['Phase', task.phase?.toString() ?? '—']),
            RetconTableRow(cells: ['Roadmap status', task.status.label]),
            RetconTableRow(cells: ['Assignee', task.assignee ?? 'Unassigned']),
            RetconTableRow(
              cells: ['Plan approved', task.planApproved ? 'Yes' : 'No'],
            ),
            RetconTableRow(
              cells: [
                'Plan steps',
                '$completeSteps / ${task.steps.length} complete',
              ],
            ),
            RetconTableRow(
              cells: [
                'Acceptance criteria',
                '$passedCriteria / ${task.criteria.length} passed',
              ],
            ),
            RetconTableRow(
              cells: [
                'Required verification',
                controller.allowsCompletion ? 'Passed' : 'Blocked',
              ],
            ),
            RetconTableRow(
              cells: [
                'Latest verification run',
                run == null ? 'Not run' : _runLabel(run.status),
              ],
            ),
            RetconTableRow(
              cells: [
                'Test results',
                run == null
                    ? '—'
                    : '${run.passedTests} passed, ${run.failedTests} failed',
              ],
            ),
            RetconTableRow(
              cells: [
                'Verification duration',
                run == null ? '—' : _duration(run.duration),
              ],
            ),
            if (report != null) ...[
              RetconTableRow(
                cells: [
                  'Files changed',
                  report.filesChanged.isEmpty
                      ? 'None recorded'
                      : report.filesChanged.join(', '),
                ],
              ),
              RetconTableRow(
                cells: [
                  'Approvals',
                  '${report.approvalsApproved} approved, '
                      '${report.approvalsDenied} denied '
                      '(${report.approvalsTotal} total)',
                ],
              ),
              RetconTableRow(cells: ['Cost', _reportCost(report)]),
              RetconTableRow(
                cells: [
                  'Report limitations',
                  report.limitations.isEmpty
                      ? 'None'
                      : report.limitations.join('; '),
                ],
              ),
            ],
          ],
        ),
      ],
    );
  }
}

class _Metric extends StatelessWidget {
  const _Metric({required this.label, required this.value});
  final String label;
  final String value;
  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Text(label, style: Theme.of(context).textTheme.labelMedium),
      Text(value, style: Theme.of(context).textTheme.titleMedium),
    ],
  );
}

Future<String?> _prompt(
  BuildContext context, {
  required String title,
  required String label,
  required String initialValue,
}) {
  var value = initialValue;
  return showDialog<String>(
    context: context,
    builder: (context) => AlertDialog(
      title: Text(title),
      content: TextFormField(
        initialValue: initialValue,
        autofocus: true,
        decoration: InputDecoration(labelText: label),
        onChanged: (next) => value = next,
        onFieldSubmitted: (next) => Navigator.of(context).pop(next.trim()),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Cancel'),
        ),
        FilledButton(
          onPressed: () => Navigator.of(context).pop(value.trim()),
          child: const Text('Save'),
        ),
      ],
    ),
  );
}

String _duration(Duration duration) => duration.inSeconds >= 1
    ? '${(duration.inMilliseconds / 1000).toStringAsFixed(1)} s'
    : '${duration.inMilliseconds} ms';

String _signed(int value) => value > 0 ? '+$value' : '$value';

String _reportCost(VerificationCompletionReport report) {
  final actual = report.actualCostMicros;
  final estimated = report.estimatedCostMicros;
  if (actual == null && estimated == null) return 'Not recorded';
  String amount(int value) =>
      '${report.currency} ${(value / 1000000).toStringAsFixed(2)}';
  if (actual == null) return '${amount(estimated!)} estimated';
  if (estimated == null) return '${amount(actual)} actual';
  return '${amount(actual)} actual / ${amount(estimated)} estimated';
}

String _gateLabel(GateStatus status) => switch (status) {
  GateStatus.queued => 'Queued',
  GateStatus.running => 'Running',
  GateStatus.passed => 'Passed',
  GateStatus.failed => 'Failed',
  GateStatus.cancelled => 'Cancelled',
  GateStatus.skipped => 'Skipped',
};

IconData _gateIcon(GateStatus status) => switch (status) {
  GateStatus.queued => Icons.schedule,
  GateStatus.running => Icons.sync,
  GateStatus.passed => Icons.check_circle,
  GateStatus.failed => Icons.cancel,
  GateStatus.cancelled => Icons.stop_circle,
  GateStatus.skipped => Icons.skip_next,
};

Color _gateColor(BuildContext context, GateStatus status) => switch (status) {
  GateStatus.passed => Colors.green,
  GateStatus.failed => Theme.of(context).colorScheme.error,
  GateStatus.running => Colors.amber,
  _ => Theme.of(context).colorScheme.onSurfaceVariant,
};

RetconStatus _gateRetconStatus(GateStatus status) => switch (status) {
  GateStatus.passed => RetconStatus.success,
  GateStatus.failed => RetconStatus.error,
  GateStatus.running || GateStatus.cancelled => RetconStatus.warning,
  _ => RetconStatus.neutral,
};

String _runLabel(VerificationRunStatus status) => switch (status) {
  VerificationRunStatus.running => 'Running',
  VerificationRunStatus.passed => 'Passed',
  VerificationRunStatus.failed => 'Failed',
  VerificationRunStatus.cancelled => 'Cancelled',
};

IconData _runIcon(VerificationRunStatus status) => switch (status) {
  VerificationRunStatus.running => Icons.sync,
  VerificationRunStatus.passed => Icons.check_circle,
  VerificationRunStatus.failed => Icons.cancel,
  VerificationRunStatus.cancelled => Icons.stop_circle,
};
