import 'package:flutter/material.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

import '../core_client.dart';
import 'checkpoint_controller.dart';

/// Minimal checkpoint list panel (Phase 18 foundation stub).
class CheckpointPanel extends StatefulWidget {
  const CheckpointPanel({
    required this.core,
    required this.root,
    super.key,
  });

  final CoreClient core;
  final String root;

  @override
  State<CheckpointPanel> createState() => _CheckpointPanelState();
}

class _CheckpointPanelState extends State<CheckpointPanel> {
  late CheckpointController _controller;

  @override
  void initState() {
    super.initState();
    _controller = CheckpointController(widget.core, root: widget.root);
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
          label: 'Checkpoints',
          padding: const EdgeInsets.all(RetconSpacing.md),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Row(
                children: [
                  RetconBadge(
                    label: '${_controller.items.length} saved',
                    status: RetconStatus.neutral,
                  ),
                  const Spacer(),
                  TextButton.icon(
                    onPressed: connected && !_controller.loading
                        ? _controller.refresh
                        : null,
                    icon: const Icon(Icons.refresh),
                    label: const Text('Refresh'),
                  ),
                ],
              ),
              const SizedBox(height: RetconSpacing.sm),
              if (!connected)
                Text(
                  'Connect to Retcon Core to load checkpoints.',
                  style: theme.textTheme.bodyMedium,
                )
              else if (_controller.loading && _controller.items.isEmpty)
                const Center(child: CircularProgressIndicator())
              else if (_controller.error != null)
                Text(
                  _controller.error!,
                  style: theme.textTheme.bodyMedium?.copyWith(
                    color: theme.colorScheme.error,
                  ),
                )
              else if (_controller.items.isEmpty)
                Text(
                  'No checkpoints yet. They appear after file writes, Git '
                  'operations, or manual capture.',
                  style: theme.textTheme.bodyMedium,
                )
              else
                Expanded(
                  child: ListView.separated(
                    itemCount: _controller.items.length,
                    separatorBuilder: (_, __) =>
                        const SizedBox(height: RetconSpacing.xs),
                    itemBuilder: (context, index) {
                      final item = _controller.items[index];
                      return ListTile(
                        contentPadding: EdgeInsets.zero,
                        leading: const Icon(Icons.restore),
                        title: Text(item.kind),
                        subtitle: Text(
                          item.turnId == null
                              ? item.id
                              : '${item.id} · turn ${item.turnId}',
                        ),
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
