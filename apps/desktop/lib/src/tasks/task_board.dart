import 'dart:async';

import 'package:flutter/material.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

import '../verification/verification.dart';
import 'task_board_controller.dart';
import 'task_models.dart';
import 'task_repository.dart';

class TaskBoardDialog extends StatelessWidget {
  const TaskBoardDialog({
    required this.repository,
    required this.verificationRepository,
    super.key,
    this.projectId,
    this.projectPath,
  });

  final TaskRepository repository;
  final VerificationRepository verificationRepository;
  final String? projectId;
  final String? projectPath;

  static Future<void> show(
    BuildContext context, {
    required TaskRepository repository,
    required VerificationRepository verificationRepository,
    String? projectId,
    String? projectPath,
  }) => showDialog<void>(
    context: context,
    builder: (context) => TaskBoardDialog(
      repository: repository,
      verificationRepository: verificationRepository,
      projectId: projectId,
      projectPath: projectPath,
    ),
  );

  @override
  Widget build(BuildContext context) => Dialog(
    insetPadding: const EdgeInsets.all(RetconSpacing.md),
    child: SizedBox(
      width: 1180,
      height: 760,
      child: Column(
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(
              RetconSpacing.md,
              RetconSpacing.sm,
              RetconSpacing.xs,
              RetconSpacing.xs,
            ),
            child: Row(
              children: [
                Text(
                  'Task board',
                  style: Theme.of(context).textTheme.titleLarge,
                ),
                const Spacer(),
                IconButton(
                  tooltip: 'Close task board',
                  onPressed: () => Navigator.of(context).pop(),
                  icon: const Icon(Icons.close),
                ),
              ],
            ),
          ),
          Expanded(
            child: TaskBoardPanel(
              repository: repository,
              verificationRepository: verificationRepository,
              projectId: projectId,
              projectPath: projectPath,
            ),
          ),
        ],
      ),
    ),
  );
}

class TaskBoardPanel extends StatefulWidget {
  const TaskBoardPanel({
    required this.repository,
    super.key,
    this.verificationRepository,
    this.projectId,
    this.projectPath,
  });

  final TaskRepository repository;
  final VerificationRepository? verificationRepository;
  final String? projectId;
  final String? projectPath;

  @override
  State<TaskBoardPanel> createState() => _TaskBoardPanelState();
}

class _TaskBoardPanelState extends State<TaskBoardPanel> {
  late final TaskBoardController _controller;
  late final VerificationRepository _verificationRepository;
  final _verificationControllers = <String, VerificationController>{};
  final _search = TextEditingController();

  @override
  void initState() {
    super.initState();
    _verificationRepository =
        widget.verificationRepository ?? InMemoryVerificationRepository.demo();
    _controller = TaskBoardController(
      repository: widget.repository,
      projectId: widget.projectId,
      verificationAllowsCompletion: (taskId) =>
          _verificationControllers[taskId]?.allowsCompletion ?? false,
    );
    unawaited(_controller.refresh());
  }

  @override
  void dispose() {
    _search.dispose();
    _controller.dispose();
    for (final controller in _verificationControllers.values) {
      controller.dispose();
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: _controller,
    builder: (context, _) => Column(
      children: [
        _TaskToolbar(
          controller: _controller,
          search: _search,
          onViewApplied: (view) {
            _controller.applyView(view);
            _search.text = view.query;
          },
        ),
        if (_controller.error != null)
          MaterialBanner(
            content: Text('Could not load tasks: ${_controller.error}'),
            actions: [
              TextButton(
                onPressed: _controller.refresh,
                child: const Text('Retry'),
              ),
            ],
          ),
        Expanded(
          child: _controller.loading && _controller.tasks.isEmpty
              ? const Center(child: CircularProgressIndicator())
              : LayoutBuilder(
                  builder: (context, constraints) {
                    final narrow = constraints.maxWidth < 900;
                    final board = _RoadmapBoard(controller: _controller);
                    final selected = _controller.selectedTask;
                    final detail = _TaskDetail(
                      controller: _controller,
                      verification: selected == null
                          ? null
                          : _verificationFor(selected),
                    );
                    if (narrow) {
                      return Row(
                        children: [
                          Expanded(child: board),
                          SizedBox(
                            width: constraints.maxWidth * .52,
                            child: detail,
                          ),
                        ],
                      );
                    }
                    return Row(
                      children: [
                        Expanded(flex: 3, child: board),
                        Expanded(flex: 2, child: detail),
                      ],
                    );
                  },
                ),
        ),
      ],
    ),
  );

  VerificationController _verificationFor(RoadmapTask task) =>
      _verificationControllers.putIfAbsent(task.id, () {
        final controller = VerificationController(
          repository: _verificationRepository,
          taskId: task.id,
          projectId: widget.projectId,
          projectPath: widget.projectPath,
        );
        unawaited(controller.load());
        return controller;
      });
}

class _TaskToolbar extends StatelessWidget {
  const _TaskToolbar({
    required this.controller,
    required this.search,
    required this.onViewApplied,
  });

  final TaskBoardController controller;
  final TextEditingController search;
  final ValueChanged<TaskSavedView> onViewApplied;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.symmetric(horizontal: RetconSpacing.md),
    child: Wrap(
      spacing: RetconSpacing.sm,
      runSpacing: RetconSpacing.xs,
      crossAxisAlignment: WrapCrossAlignment.center,
      children: [
        SizedBox(
          width: 260,
          child: TextField(
            key: const Key('task-search'),
            controller: search,
            onChanged: controller.setQuery,
            decoration: const InputDecoration(
              labelText: 'Search tasks',
              prefixIcon: Icon(Icons.search),
            ),
          ),
        ),
        PopupMenuButton<TaskStatus>(
          tooltip: 'Filter by status',
          onSelected: controller.toggleStatus,
          itemBuilder: (context) => [
            for (final status in TaskStatus.values)
              CheckedPopupMenuItem(
                value: status,
                checked: controller.statuses.contains(status),
                child: Text(status.label),
              ),
          ],
          child: Chip(
            avatar: const Icon(Icons.filter_list, size: 18),
            label: Text(
              controller.statuses.isEmpty
                  ? 'All statuses'
                  : '${controller.statuses.length} statuses',
            ),
          ),
        ),
        DropdownButton<TaskGrouping>(
          value: controller.grouping,
          onChanged: (value) {
            if (value != null) controller.setGrouping(value);
          },
          items: [
            for (final value in TaskGrouping.values)
              DropdownMenuItem(
                value: value,
                child: Text('Group: ${value.label}'),
              ),
          ],
        ),
        if (controller.savedViews.isNotEmpty)
          PopupMenuButton<TaskSavedView>(
            tooltip: 'Saved views',
            onSelected: onViewApplied,
            itemBuilder: (context) => [
              for (final view in controller.savedViews)
                PopupMenuItem(value: view, child: Text(view.name)),
            ],
            child: const Chip(
              avatar: Icon(Icons.bookmarks_outlined, size: 18),
              label: Text('Saved views'),
            ),
          ),
        TextButton.icon(
          onPressed: () => _createTask(context),
          icon: const Icon(Icons.add),
          label: const Text('New task'),
        ),
        TextButton.icon(
          onPressed: () => _saveView(context),
          icon: const Icon(Icons.bookmark_add_outlined),
          label: const Text('Save view'),
        ),
        TextButton.icon(
          onPressed: controller.refresh,
          icon: const Icon(Icons.refresh),
          label: const Text('Refresh'),
        ),
      ],
    ),
  );

  Future<void> _createTask(BuildContext context) async {
    final title = await _prompt(
      context,
      title: 'New task',
      label: 'Task title',
    );
    if (title != null) await controller.createTask(title);
  }

  Future<void> _saveView(BuildContext context) async {
    final name = await _prompt(
      context,
      title: 'Save current view',
      label: 'View name',
    );
    if (name != null) await controller.saveCurrentView(name);
  }
}

class _RoadmapBoard extends StatelessWidget {
  const _RoadmapBoard({required this.controller});
  final TaskBoardController controller;

  @override
  Widget build(BuildContext context) {
    final groups = controller.groupedTasks;
    if (groups.isEmpty) {
      return const Center(child: Text('No tasks match this view.'));
    }
    return Semantics(
      label: 'Roadmap task groups',
      child: ListView(
        scrollDirection: Axis.horizontal,
        padding: const EdgeInsets.all(RetconSpacing.md),
        children: [
          for (final entry in groups.entries)
            SizedBox(
              width: 250,
              child: Padding(
                padding: const EdgeInsets.only(right: RetconSpacing.sm),
                child: RetconPanel(
                  label: entry.key,
                  recessed: true,
                  padding: const EdgeInsets.all(RetconSpacing.sm),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Row(
                        children: [
                          Expanded(
                            child: Text(
                              entry.key,
                              style: Theme.of(context).textTheme.titleSmall,
                            ),
                          ),
                          RetconBadge(label: '${entry.value.length}'),
                        ],
                      ),
                      const SizedBox(height: RetconSpacing.sm),
                      Expanded(
                        child: ListView.separated(
                          itemCount: entry.value.length,
                          separatorBuilder: (_, _) =>
                              const SizedBox(height: RetconSpacing.sm),
                          itemBuilder: (context, index) {
                            final task = entry.value[index];
                            return _TaskCard(
                              task: task,
                              selected: controller.selectedTask?.id == task.id,
                              onTap: () => controller.selectTask(task.id),
                            );
                          },
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}

class _TaskCard extends StatelessWidget {
  const _TaskCard({
    required this.task,
    required this.selected,
    required this.onTap,
  });
  final RoadmapTask task;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => Material(
    color: selected ? Theme.of(context).colorScheme.primaryContainer : null,
    child: InkWell(
      key: Key('task-card-${task.id}'),
      onTap: onTap,
      child: Padding(
        padding: const EdgeInsets.all(RetconSpacing.sm),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(task.title, style: Theme.of(context).textTheme.titleSmall),
            const SizedBox(height: RetconSpacing.xs),
            Wrap(
              spacing: RetconSpacing.xs,
              runSpacing: RetconSpacing.xs,
              children: [
                if (task.phase != null)
                  RetconBadge(label: 'Phase ${task.phase}'),
                RetconBadge(
                  label: task.status.label,
                  status: _taskStatus(task.status),
                ),
              ],
            ),
            if (task.assignee != null) ...[
              const SizedBox(height: RetconSpacing.xs),
              Text(
                task.assignee!,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
              ),
            ],
          ],
        ),
      ),
    ),
  );
}

class _TaskDetail extends StatelessWidget {
  const _TaskDetail({required this.controller, required this.verification});
  final TaskBoardController controller;
  final VerificationController? verification;

  @override
  Widget build(BuildContext context) {
    final task = controller.selectedTask;
    if (task == null) {
      return const RetconPanel(
        child: Center(child: Text('Select a task to edit its plan.')),
      );
    }
    final verification = this.verification!;
    return AnimatedBuilder(
      animation: verification,
      builder: (context, _) => RetconPanel(
        key: Key('task-detail-${task.id}'),
        label: 'Task plan',
        padding: const EdgeInsets.all(RetconSpacing.md),
        child: ListView(
          children: [
            Text(task.title, style: Theme.of(context).textTheme.titleLarge),
            const SizedBox(height: RetconSpacing.xs),
            Wrap(
              spacing: RetconSpacing.xs,
              runSpacing: RetconSpacing.xs,
              children: [
                RetconBadge(
                  label: task.status.label,
                  status: _taskStatus(task.status),
                ),
                RetconBadge(
                  label: task.planApproved
                      ? 'Plan approved'
                      : 'Plan awaiting approval',
                  status: task.planApproved
                      ? RetconStatus.success
                      : RetconStatus.warning,
                ),
              ],
            ),
            const SizedBox(height: RetconSpacing.sm),
            Wrap(
              spacing: RetconSpacing.xs,
              runSpacing: RetconSpacing.xs,
              children: [
                FilledButton.icon(
                  key: const Key('approve-plan'),
                  onPressed: task.planApproved
                      ? null
                      : () => controller.approvePlan(true),
                  icon: const Icon(Icons.approval),
                  label: const Text('Approve plan'),
                ),
                OutlinedButton.icon(
                  onPressed: task.status == TaskStatus.inProgress
                      ? () => controller.setTaskStatus(TaskStatus.paused)
                      : () => controller.setTaskStatus(TaskStatus.inProgress),
                  icon: Icon(
                    task.status == TaskStatus.inProgress
                        ? Icons.pause
                        : Icons.play_arrow,
                  ),
                  label: Text(
                    task.status == TaskStatus.inProgress ? 'Pause' : 'Resume',
                  ),
                ),
                OutlinedButton.icon(
                  onPressed: () => controller.setTaskStatus(TaskStatus.blocked),
                  icon: const Icon(Icons.block),
                  label: const Text('Block'),
                ),
              ],
            ),
            const Divider(height: RetconSpacing.lg),
            Row(
              children: [
                Expanded(
                  child: Text(
                    'Plan steps',
                    style: Theme.of(context).textTheme.titleMedium,
                  ),
                ),
                IconButton(
                  key: const Key('add-step'),
                  tooltip: 'Add plan step',
                  onPressed: () => _addStep(context),
                  icon: const Icon(Icons.add),
                ),
              ],
            ),
            if (task.steps.isEmpty) const Text('No plan steps yet.'),
            for (var index = 0; index < task.steps.length; index++)
              _PlanStepTile(
                step: task.steps[index],
                index: index,
                count: task.steps.length,
                controller: controller,
              ),
            const Divider(height: RetconSpacing.lg),
            Text(
              'Acceptance criteria',
              style: Theme.of(context).textTheme.titleMedium,
            ),
            const SizedBox(height: RetconSpacing.xs),
            if (task.criteria.isEmpty)
              const Text('No acceptance criteria defined.'),
            for (final criterion in task.criteria)
              _CriterionTile(criterion: criterion, controller: controller),
            const Divider(height: RetconSpacing.lg),
            TaskVerificationSection(controller: verification, task: task),
            const SizedBox(height: RetconSpacing.md),
            if (!task.canComplete || !verification.allowsCompletion)
              Text(
                _gateMessage(task, verification),
                key: const Key('completion-gate-message'),
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            const SizedBox(height: RetconSpacing.sm),
            FilledButton.icon(
              key: const Key('complete-task'),
              onPressed: task.canComplete && verification.allowsCompletion
                  ? () => controller.setTaskStatus(TaskStatus.complete)
                  : null,
              icon: const Icon(Icons.task_alt),
              label: const Text('Complete task'),
            ),
          ],
        ),
      ),
    );
  }

  Future<void> _addStep(BuildContext context) async {
    final title = await _prompt(
      context,
      title: 'Add plan step',
      label: 'Step title',
    );
    if (title != null) await controller.addStep(title);
  }
}

class _PlanStepTile extends StatelessWidget {
  const _PlanStepTile({
    required this.step,
    required this.index,
    required this.count,
    required this.controller,
  });

  final PlanStep step;
  final int index;
  final int count;
  final TaskBoardController controller;

  @override
  Widget build(BuildContext context) => Card(
    child: Padding(
      padding: const EdgeInsets.all(RetconSpacing.xs),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              PopupMenuButton<PlanStepStatus>(
                key: Key('step-status-${step.id}'),
                tooltip: 'Change step status',
                onSelected: (status) =>
                    controller.updateStep(step.id, status: status),
                itemBuilder: (_) => [
                  for (final status in PlanStepStatus.values)
                    PopupMenuItem(value: status, child: Text(status.label)),
                ],
                child: Icon(_stepIcon(step.status)),
              ),
              Expanded(child: Text(step.title)),
              IconButton(
                tooltip: 'Edit step',
                onPressed: () => _edit(context),
                icon: const Icon(Icons.edit_outlined),
              ),
              IconButton(
                tooltip: 'Move step up',
                onPressed: index == 0
                    ? null
                    : () => controller.moveStep(step.id, -1),
                icon: const Icon(Icons.arrow_upward),
              ),
              IconButton(
                tooltip: 'Move step down',
                onPressed: index == count - 1
                    ? null
                    : () => controller.moveStep(step.id, 1),
                icon: const Icon(Icons.arrow_downward),
              ),
              IconButton(
                tooltip: 'Delete step',
                onPressed: () => controller.deleteStep(step.id),
                icon: const Icon(Icons.delete_outline),
              ),
            ],
          ),
          Wrap(
            spacing: RetconSpacing.xs,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              Text(step.status.label),
              TextButton.icon(
                onPressed: () => _assign(context),
                icon: const Icon(Icons.person_outline),
                label: Text(step.assignee ?? 'Assign'),
              ),
              TextButton.icon(
                onPressed: () => _evidence(context),
                icon: const Icon(Icons.attach_file),
                label: Text(
                  step.evidence.isEmpty
                      ? 'Add evidence'
                      : '${step.evidence.length} evidence',
                ),
              ),
            ],
          ),
        ],
      ),
    ),
  );

  Future<void> _edit(BuildContext context) async {
    final title = await _prompt(
      context,
      title: 'Edit plan step',
      label: 'Step title',
      initialValue: step.title,
    );
    if (title != null && title.trim().isNotEmpty) {
      await controller.updateStep(step.id, title: title.trim());
    }
  }

  Future<void> _assign(BuildContext context) async {
    final assignee = await _prompt(
      context,
      title: 'Assign plan step',
      label: 'Assignee',
      initialValue: step.assignee,
    );
    if (assignee != null && assignee.trim().isNotEmpty) {
      await controller.updateStep(step.id, assignee: assignee.trim());
    }
  }

  Future<void> _evidence(BuildContext context) async {
    final evidence = await _prompt(
      context,
      title: 'Attach evidence',
      label: 'Evidence link or summary',
    );
    if (evidence != null) {
      await controller.updateStep(step.id, evidence: evidence);
    }
  }
}

class _CriterionTile extends StatelessWidget {
  const _CriterionTile({required this.criterion, required this.controller});
  final AcceptanceCriterion criterion;
  final TaskBoardController controller;

  @override
  Widget build(BuildContext context) => CheckboxListTile(
    key: Key('criterion-${criterion.id}'),
    contentPadding: EdgeInsets.zero,
    value: criterion.status == CriterionStatus.passed,
    onChanged: (passed) => controller.setCriterionStatus(
      criterion.id,
      passed == true ? CriterionStatus.passed : CriterionStatus.failed,
    ),
    secondary: criterion.status == CriterionStatus.failed
        ? Icon(Icons.error, color: Theme.of(context).colorScheme.error)
        : null,
    title: Text(criterion.title),
    subtitle: Text(criterion.required ? 'Required' : 'Optional'),
  );
}

String _gateMessage(RoadmapTask task, VerificationController verification) {
  final missing = <String>[];
  if (!task.planApproved) missing.add('approve the plan');
  if (!task.stepsComplete) missing.add('complete every plan step');
  if (!task.acceptanceCriteriaMet) {
    missing.add('pass all required acceptance criteria');
  }
  if (!verification.allowsCompletion) {
    final blocker =
        verification.completionBlocker ?? 'pass required verification gates';
    missing.add(
      blocker.endsWith('.')
          ? blocker.substring(0, blocker.length - 1)
          : blocker,
    );
  }
  return 'Completion blocked: ${missing.join(', ')}.';
}

RetconStatus _taskStatus(TaskStatus status) => switch (status) {
  TaskStatus.complete => RetconStatus.success,
  TaskStatus.blocked => RetconStatus.error,
  TaskStatus.paused || TaskStatus.review => RetconStatus.warning,
  _ => RetconStatus.neutral,
};

IconData _stepIcon(PlanStepStatus status) => switch (status) {
  PlanStepStatus.pending => Icons.radio_button_unchecked,
  PlanStepStatus.inProgress => Icons.play_circle_outline,
  PlanStepStatus.paused => Icons.pause_circle_outline,
  PlanStepStatus.blocked => Icons.block,
  PlanStepStatus.complete => Icons.check_circle,
};

Future<String?> _prompt(
  BuildContext context, {
  required String title,
  required String label,
  String? initialValue,
}) async {
  var value = initialValue ?? '';
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
