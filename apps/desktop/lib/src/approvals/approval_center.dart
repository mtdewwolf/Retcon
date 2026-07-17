import 'package:flutter/material.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

import '../core_client.dart';
import 'approval_controller.dart';

/// Approval center panel for reviewing and resolving pending requests.
class ApprovalCenterPanel extends StatefulWidget {
  const ApprovalCenterPanel({required this.core, super.key});

  final CoreClient core;

  @override
  State<ApprovalCenterPanel> createState() => _ApprovalCenterPanelState();
}

class _ApprovalCenterPanelState extends State<ApprovalCenterPanel> {
  late ApprovalController _controller;

  @override
  void initState() {
    super.initState();
    _controller = ApprovalController(widget.core);
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return AnimatedBuilder(
      animation: Listenable.merge([_controller, widget.core]),
      builder: (context, _) {
        final connected =
            widget.core.status == CoreConnectionStatus.connected;
        return RetconPanel(
          label: 'Approval center',
          padding: const EdgeInsets.all(RetconSpacing.md),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Row(
                children: [
                  RetconBadge(
                    label: '${_controller.pendingCount} pending',
                    status: _controller.pendingCount == 0
                        ? RetconStatus.success
                        : RetconStatus.warning,
                  ),
                  const Spacer(),
                  TextButton.icon(
                    onPressed:
                        connected && !_controller.loading ? _controller.refresh : null,
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
              if (_controller.error != null) ...[
                const SizedBox(height: RetconSpacing.sm),
                Text(
                  _controller.error!,
                  style: theme.textTheme.bodySmall?.copyWith(
                    color: theme.colorScheme.error,
                  ),
                ),
              ],
              const SizedBox(height: RetconSpacing.sm),
              Expanded(
                child: _controller.loading && _controller.items.isEmpty
                    ? const Center(child: CircularProgressIndicator())
                    : _controller.items.isEmpty
                    ? Center(
                        child: Text(
                          connected
                              ? 'No pending approvals.'
                              : 'Approval queue unavailable.',
                          style: theme.textTheme.bodyLarge,
                        ),
                      )
                    : ListView.separated(
                        itemCount: _controller.items.length,
                        separatorBuilder: (_, __) =>
                            const SizedBox(height: RetconSpacing.sm),
                        itemBuilder: (context, index) {
                          final item = _controller.items[index];
                          return _ApprovalCard(
                            item: item,
                            onApprove: () => _controller.decide(
                              item.id,
                              'approve',
                              remember: 'once',
                            ),
                            onApproveAlways: () => _controller.decide(
                              item.id,
                              'approve',
                              remember: 'always',
                            ),
                            onDeny: () =>
                                _controller.decide(item.id, 'deny'),
                          );
                        },
                      ),
              ),
            ],
          ),
        );
      },
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
