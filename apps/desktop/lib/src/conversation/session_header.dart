import 'package:flutter/material.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

import 'conversation_models.dart';

/// Session status, token usage, and pending approvals.
class SessionHeader extends StatelessWidget {
  const SessionHeader({
    required this.state,
    required this.usage,
    required this.pendingApprovals,
    super.key,
    this.error,
    this.providerLabel,
    this.onApprove,
    this.onDeny,
  });

  final ConversationSessionState state;
  final TokenUsage usage;
  final List<PendingApproval> pendingApprovals;
  final String? error;
  final String? providerLabel;
  final ValueChanged<PendingApproval>? onApprove;
  final ValueChanged<PendingApproval>? onDeny;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return RetconPanel(
      label: 'Agent session',
      padding: const EdgeInsets.symmetric(
        horizontal: RetconSpacing.md,
        vertical: RetconSpacing.sm,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Wrap(
            spacing: RetconSpacing.xs,
            runSpacing: RetconSpacing.xs,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              RetconBadge(
                label: _stateLabel(state),
                status: _stateStatus(state),
              ),
              if (providerLabel != null) RetconBadge(label: providerLabel!),
              RetconBadge(label: '${usage.totalTokens} tokens'),
              if (usage.costUsd != null)
                RetconBadge(label: '\$${usage.costUsd!.toStringAsFixed(4)}'),
              RetconBadge(
                label: '${pendingApprovals.length} approvals',
                status: pendingApprovals.isEmpty
                    ? RetconStatus.neutral
                    : RetconStatus.warning,
              ),
            ],
          ),
          if (error != null) ...[
            const SizedBox(height: RetconSpacing.sm),
            Text(
              error!,
              style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.error,
              ),
            ),
          ],
          if (pendingApprovals.isNotEmpty) ...[
            const SizedBox(height: RetconSpacing.sm),
            for (final approval in pendingApprovals)
              Padding(
                padding: const EdgeInsets.only(bottom: RetconSpacing.xs),
                child: RetconPanel(
                  recessed: true,
                  label: approval.title,
                  padding: const EdgeInsets.all(RetconSpacing.sm),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(approval.title, style: theme.textTheme.titleSmall),
                      if (approval.detail != null) ...[
                        const SizedBox(height: RetconSpacing.xxs),
                        Text(approval.detail!),
                      ],
                      if (onApprove != null || onDeny != null) ...[
                        const SizedBox(height: RetconSpacing.sm),
                        Wrap(
                          spacing: RetconSpacing.xs,
                          runSpacing: RetconSpacing.xs,
                          children: [
                            if (onApprove != null)
                              FilledButton(
                                onPressed: () => onApprove!(approval),
                                child: const Text('Approve'),
                              ),
                            if (onDeny != null)
                              TextButton(
                                onPressed: () => onDeny!(approval),
                                child: const Text('Deny'),
                              ),
                          ],
                        ),
                      ],
                    ],
                  ),
                ),
              ),
          ],
        ],
      ),
    );
  }

  static String _stateLabel(ConversationSessionState state) => switch (state) {
    ConversationSessionState.idle => 'Idle',
    ConversationSessionState.preparing => 'Preparing',
    ConversationSessionState.running => 'Running',
    ConversationSessionState.waitingApproval => 'Awaiting approval',
    ConversationSessionState.paused => 'Paused',
    ConversationSessionState.completed => 'Completed',
    ConversationSessionState.failed => 'Failed',
    ConversationSessionState.cancelled => 'Cancelled',
  };

  static RetconStatus _stateStatus(ConversationSessionState state) =>
      switch (state) {
        ConversationSessionState.running => RetconStatus.success,
        ConversationSessionState.preparing => RetconStatus.warning,
        ConversationSessionState.waitingApproval => RetconStatus.warning,
        ConversationSessionState.failed => RetconStatus.error,
        ConversationSessionState.cancelled => RetconStatus.warning,
        ConversationSessionState.completed => RetconStatus.success,
        _ => RetconStatus.neutral,
      };
}
