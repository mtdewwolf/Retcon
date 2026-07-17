import 'package:flutter/material.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

import '../core_client.dart';
import 'approval_controller.dart';
import 'permission_rules_controller.dart';

enum _ApprovalCenterTab { pending, rules }

/// Approval center panel for reviewing requests and editing permission rules.
class ApprovalCenterPanel extends StatefulWidget {
  const ApprovalCenterPanel({
    required this.core,
    this.projectId,
    super.key,
  });

  final CoreClient core;
  final String? projectId;

  @override
  State<ApprovalCenterPanel> createState() => _ApprovalCenterPanelState();
}

class _ApprovalCenterPanelState extends State<ApprovalCenterPanel> {
  late ApprovalController _approvals;
  late PermissionRulesController _rules;
  _ApprovalCenterTab _tab = _ApprovalCenterTab.pending;

  @override
  void initState() {
    super.initState();
    _approvals = ApprovalController(widget.core);
    _rules = PermissionRulesController(
      widget.core,
      projectId: widget.projectId,
    );
  }

  @override
  void didUpdateWidget(covariant ApprovalCenterPanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.projectId != widget.projectId) {
      _rules.updateProjectId(widget.projectId);
    }
    if (oldWidget.core != widget.core) {
      _approvals.dispose();
      _rules.dispose();
      _approvals = ApprovalController(widget.core);
      _rules = PermissionRulesController(
        widget.core,
        projectId: widget.projectId,
      );
    }
  }

  @override
  void dispose() {
    _approvals.dispose();
    _rules.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return AnimatedBuilder(
      animation: Listenable.merge([_approvals, _rules, widget.core]),
      builder: (context, _) {
        final connected =
            widget.core.status == CoreConnectionStatus.connected;
        return RetconPanel(
          label: 'Approval center',
          padding: const EdgeInsets.all(RetconSpacing.md),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              SegmentedButton<_ApprovalCenterTab>(
                segments: const [
                  ButtonSegment(
                    value: _ApprovalCenterTab.pending,
                    label: Text('Pending'),
                    icon: Icon(Icons.pending_actions),
                  ),
                  ButtonSegment(
                    value: _ApprovalCenterTab.rules,
                    label: Text('Rules'),
                    icon: Icon(Icons.rule),
                  ),
                ],
                selected: {_tab},
                onSelectionChanged: (value) {
                  setState(() => _tab = value.first);
                },
              ),
              const SizedBox(height: RetconSpacing.sm),
              if (_tab == _ApprovalCenterTab.pending)
                Expanded(
                  child: _PendingApprovalsView(
                    controller: _approvals,
                    connected: connected,
                    theme: theme,
                  ),
                )
              else
                Expanded(
                  child: _PermissionRulesView(
                    controller: _rules,
                    connected: connected,
                    theme: theme,
                  ),
                ),
            ],
          ),
        );
      },
    );
  }
}

class _PendingApprovalsView extends StatelessWidget {
  const _PendingApprovalsView({
    required this.controller,
    required this.connected,
    required this.theme,
  });

  final ApprovalController controller;
  final bool connected;
  final ThemeData theme;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            RetconBadge(
              label: '${controller.pendingCount} pending',
              status: controller.pendingCount == 0
                  ? RetconStatus.success
                  : RetconStatus.warning,
            ),
            const Spacer(),
            TextButton.icon(
              onPressed: connected && !controller.loading
                  ? controller.refresh
                  : null,
              icon: const Icon(Icons.refresh),
              label: const Text('Refresh'),
            ),
          ],
        ),
        if (!connected) ...[
          const SizedBox(height: RetconSpacing.sm),
          Text(
            'Connect to Retcon Core to review approvals.',
            style: theme.textTheme.bodyMedium,
          ),
        ],
        if (controller.error != null) ...[
          const SizedBox(height: RetconSpacing.sm),
          Text(
            controller.error!,
            style: theme.textTheme.bodySmall?.copyWith(
              color: theme.colorScheme.error,
            ),
          ),
        ],
        const SizedBox(height: RetconSpacing.sm),
        Expanded(
          child: controller.loading && controller.items.isEmpty
              ? const Center(child: CircularProgressIndicator())
              : controller.items.isEmpty
              ? Center(
                  child: Text(
                    connected
                        ? 'No pending approvals.'
                        : 'Approval queue unavailable.',
                    style: theme.textTheme.bodyLarge,
                  ),
                )
              : ListView.separated(
                  itemCount: controller.items.length,
                  separatorBuilder: (_, _) =>
                      const SizedBox(height: RetconSpacing.sm),
                  itemBuilder: (context, index) {
                    final item = controller.items[index];
                    return _ApprovalCard(
                      item: item,
                      onApprove: () => controller.decide(
                        item.id,
                        'approve',
                        remember: 'once',
                      ),
                      onApproveAlways: () => controller.decide(
                        item.id,
                        'approve',
                        remember: 'always',
                      ),
                      onDeny: () => controller.decide(item.id, 'deny'),
                    );
                  },
                ),
        ),
      ],
    );
  }
}

class _PermissionRulesView extends StatefulWidget {
  const _PermissionRulesView({
    required this.controller,
    required this.connected,
    required this.theme,
  });

  final PermissionRulesController controller;
  final bool connected;
  final ThemeData theme;

  @override
  State<_PermissionRulesView> createState() => _PermissionRulesViewState();
}

class _PermissionRulesViewState extends State<_PermissionRulesView> {
  final _method = TextEditingController();
  String _effect = 'allow';

  @override
  void dispose() {
    _method.dispose();
    super.dispose();
  }

  Future<void> _create() async {
    await widget.controller.createRule(
      effect: _effect,
      method: _method.text,
    );
    if (widget.controller.error == null) {
      _method.clear();
    }
  }

  @override
  Widget build(BuildContext context) {
    final controller = widget.controller;
    final theme = widget.theme;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            RetconBadge(
              label: '${controller.rules.length} rules',
              status: RetconStatus.neutral,
            ),
            const Spacer(),
            TextButton.icon(
              onPressed: widget.connected && !controller.loading
                  ? controller.refresh
                  : null,
              icon: const Icon(Icons.refresh),
              label: const Text('Refresh'),
            ),
          ],
        ),
        const SizedBox(height: RetconSpacing.sm),
        if (widget.connected) ...[
          Row(
            crossAxisAlignment: CrossAxisAlignment.end,
            children: [
              Expanded(
                flex: 2,
                child: RetconTextField(
                  label: 'RPC method',
                  hint: 'file.write',
                  controller: _method,
                ),
              ),
              const SizedBox(width: RetconSpacing.sm),
              DropdownButton<String>(
                value: _effect,
                items: const [
                  DropdownMenuItem(value: 'allow', child: Text('Allow')),
                  DropdownMenuItem(value: 'deny', child: Text('Deny')),
                ],
                onChanged: (value) {
                  if (value != null) setState(() => _effect = value);
                },
              ),
              const SizedBox(width: RetconSpacing.sm),
              FilledButton(
                onPressed: controller.mutating ? null : _create,
                child: const Text('Add rule'),
              ),
            ],
          ),
          const SizedBox(height: RetconSpacing.sm),
        ],
        if (controller.error != null) ...[
          Text(
            controller.error!,
            style: theme.textTheme.bodySmall?.copyWith(
              color: theme.colorScheme.error,
            ),
          ),
          const SizedBox(height: RetconSpacing.sm),
        ],
        Expanded(
          child: !widget.connected
              ? Text(
                  'Connect to Retcon Core to manage permission rules.',
                  style: theme.textTheme.bodyMedium,
                )
              : controller.loading && controller.rules.isEmpty
              ? const Center(child: CircularProgressIndicator())
              : controller.rules.isEmpty
              ? Text(
                  controller.projectId == null
                      ? 'Open a project to list permission rules.'
                      : 'No permission rules yet. Add one above, or use '
                            'Always allow on a pending approval.',
                  style: theme.textTheme.bodyMedium,
                )
              : ListView.separated(
                  itemCount: controller.rules.length,
                  separatorBuilder: (_, _) =>
                      const SizedBox(height: RetconSpacing.sm),
                  itemBuilder: (context, index) {
                    final rule = controller.rules[index];
                    return RetconPanel(
                      recessed: true,
                      label: rule.effect,
                      padding: const EdgeInsets.all(RetconSpacing.sm),
                      child: Row(
                        children: [
                          Expanded(
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Text(
                                  rule.matcherLabel,
                                  style: theme.textTheme.titleSmall,
                                ),
                                Text(
                                  '${rule.scope}'
                                  '${rule.projectId == null ? ' · global' : ''}',
                                  style: theme.textTheme.bodySmall,
                                ),
                              ],
                            ),
                          ),
                          IconButton(
                            tooltip: 'Delete rule',
                            onPressed: controller.mutating
                                ? null
                                : () => controller.deleteRule(rule.id),
                            icon: const Icon(Icons.delete_outline),
                          ),
                        ],
                      ),
                    );
                  },
                ),
        ),
      ],
    );
  }
}

class _ApprovalCard extends StatelessWidget {
  const _ApprovalCard({
    required this.item,
    required this.onApprove,
    required this.onApproveAlways,
    required this.onDeny,
  });

  final ApprovalItem item;
  final VoidCallback onApprove;
  final VoidCallback onApproveAlways;
  final VoidCallback onDeny;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return RetconPanel(
      recessed: true,
      label: item.category ?? 'Approval',
      padding: const EdgeInsets.all(RetconSpacing.sm),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(item.title, style: theme.textTheme.titleSmall),
          if (item.detail != null) ...[
            const SizedBox(height: RetconSpacing.xxs),
            Text(item.detail!, style: theme.textTheme.bodySmall),
          ],
          const SizedBox(height: RetconSpacing.sm),
          Wrap(
            spacing: RetconSpacing.xs,
            runSpacing: RetconSpacing.xs,
            children: [
              FilledButton(onPressed: onApprove, child: const Text('Approve')),
              OutlinedButton(
                onPressed: onApproveAlways,
                child: const Text('Always allow'),
              ),
              TextButton(onPressed: onDeny, child: const Text('Deny')),
            ],
          ),
        ],
      ),
    );
  }
}
