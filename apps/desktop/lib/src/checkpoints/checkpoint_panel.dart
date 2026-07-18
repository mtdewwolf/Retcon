import 'package:flutter/material.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

import '../core_client.dart';
import 'checkpoint_controller.dart';

/// Checkpoint history with detail, preview, and selective restore.
class CheckpointPanel extends StatefulWidget {
  const CheckpointPanel({required this.core, required this.root, super.key});

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
  void didUpdateWidget(covariant CheckpointPanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.root != widget.root || oldWidget.core != widget.core) {
      _controller.dispose();
      _controller = CheckpointController(widget.core, root: widget.root);
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  Future<void> _confirmRestore() async {
    final paths = _controller.selectedPaths.toList()..sort();
    if (paths.isEmpty) return;
    final force = paths.any((path) {
      final preview = _controller.detail?.previewItems;
      if (preview == null) return false;
      return preview.any((item) => item.path == path && item.conflict);
    });
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Restore checkpoint'),
        content: Text(
          force
              ? 'Restore ${paths.length} path(s), including conflicted files?'
              : 'Restore ${paths.length} selected path(s) from this checkpoint?',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: Text(force ? 'Force restore' : 'Restore'),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    final report = await _controller.restoreSelected(force: force);
    if (!mounted || report == null) return;
    final message = StringBuffer('Restored ${report.restored.length}');
    if (report.skipped.isNotEmpty) {
      message.write(', skipped ${report.skipped.length}');
    }
    if (report.conflicts.isNotEmpty) {
      message.write(', conflicts ${report.conflicts.length}');
    }
    ScaffoldMessenger.of(
      context,
    ).showSnackBar(SnackBar(content: Text(message.toString())));
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return AnimatedBuilder(
      animation: Listenable.merge([_controller, widget.core]),
      builder: (context, _) {
        final connected = widget.core.status == CoreConnectionStatus.connected;
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
                    onPressed: connected && !_controller.mutating
                        ? () => _controller.createManual()
                        : null,
                    icon: const Icon(Icons.add),
                    label: const Text('Create'),
                  ),
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
                const Expanded(
                  child: Center(child: CircularProgressIndicator()),
                )
              else if (_controller.error != null && _controller.items.isEmpty)
                Text(
                  _controller.error!,
                  style: theme.textTheme.bodyMedium?.copyWith(
                    color: theme.colorScheme.error,
                  ),
                )
              else if (_controller.items.isEmpty)
                Text(
                  'No checkpoints yet. Create one manually, or they appear '
                  'after file writes and Git operations.',
                  style: theme.textTheme.bodyMedium,
                )
              else
                Expanded(
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Expanded(
                        flex: 2,
                        child: _CheckpointList(controller: _controller),
                      ),
                      const SizedBox(width: RetconSpacing.md),
                      Expanded(
                        flex: 3,
                        child: _CheckpointDetailPane(
                          controller: _controller,
                          onPreview: _controller.previewSelected,
                          onRestore: _confirmRestore,
                        ),
                      ),
                    ],
                  ),
                ),
            ],
          ),
        );
      },
    );
  }
}

class _CheckpointList extends StatelessWidget {
  const _CheckpointList({required this.controller});

  final CheckpointController controller;

  @override
  Widget build(BuildContext context) {
    return ListView.separated(
      itemCount: controller.items.length,
      separatorBuilder: (_, _) => const SizedBox(height: RetconSpacing.xs),
      itemBuilder: (context, index) {
        final item = controller.items[index];
        final selected = controller.selectedId == item.id;
        return Material(
          color: selected
              ? Theme.of(context).colorScheme.primaryContainer
              : Colors.transparent,
          child: ListTile(
            contentPadding: const EdgeInsets.symmetric(
              horizontal: RetconSpacing.sm,
            ),
            leading: const Icon(Icons.restore),
            selected: selected,
            title: Text(item.kind),
            subtitle: Text(
              item.turnId == null
                  ? item.id
                  : '${item.id} · turn ${item.turnId}',
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
            ),
            onTap: () => controller.select(item.id),
          ),
        );
      },
    );
  }
}

class _CheckpointDetailPane extends StatelessWidget {
  const _CheckpointDetailPane({
    required this.controller,
    required this.onPreview,
    required this.onRestore,
  });

  final CheckpointController controller;
  final VoidCallback onPreview;
  final VoidCallback onRestore;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final detail = controller.detail;

    if (controller.detailLoading && detail == null) {
      return const Center(child: CircularProgressIndicator());
    }
    if (detail == null) {
      return Center(
        child: Text(
          controller.detailError ?? 'Select a checkpoint to inspect.',
          style: theme.textTheme.bodyMedium?.copyWith(
            color: controller.detailError != null
                ? theme.colorScheme.error
                : null,
          ),
          textAlign: TextAlign.center,
        ),
      );
    }

    final paths = detail.previewItems.isNotEmpty
        ? detail.previewItems.map((item) => item.path).toList()
        : detail.fileChanges.map((change) => change.path).toList();

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(detail.checkpoint.kind, style: theme.textTheme.titleMedium),
        Text(
          detail.checkpoint.id,
          style: theme.textTheme.bodySmall,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
        ),
        if (controller.detailError != null) ...[
          const SizedBox(height: RetconSpacing.xs),
          Text(
            controller.detailError!,
            style: theme.textTheme.bodySmall?.copyWith(
              color: theme.colorScheme.error,
            ),
          ),
        ],
        if (controller.lastRestore != null) ...[
          const SizedBox(height: RetconSpacing.xs),
          Text(
            'Last restore: ${controller.lastRestore!.restored.length} restored',
            style: theme.textTheme.bodySmall,
          ),
        ],
        const SizedBox(height: RetconSpacing.sm),
        Wrap(
          spacing: RetconSpacing.xs,
          runSpacing: RetconSpacing.xs,
          children: [
            TextButton.icon(
              onPressed: controller.detailLoading || controller.mutating
                  ? null
                  : onPreview,
              icon: const Icon(Icons.preview),
              label: const Text('Preview'),
            ),
            TextButton(
              onPressed: paths.isEmpty
                  ? null
                  : () => controller.selectAllPaths(paths),
              child: const Text('Select all'),
            ),
            TextButton(
              onPressed: controller.selectedPaths.isEmpty
                  ? null
                  : controller.clearPathSelection,
              child: const Text('Clear'),
            ),
            FilledButton.icon(
              onPressed: controller.selectedPaths.isEmpty || controller.mutating
                  ? null
                  : onRestore,
              icon: const Icon(Icons.undo),
              label: Text('Restore (${controller.selectedPaths.length})'),
            ),
          ],
        ),
        const SizedBox(height: RetconSpacing.sm),
        Expanded(
          child: paths.isEmpty
              ? Text(
                  'No file changes recorded for this checkpoint.',
                  style: theme.textTheme.bodyMedium,
                )
              : ListView.separated(
                  itemCount: paths.length,
                  separatorBuilder: (_, _) =>
                      const SizedBox(height: RetconSpacing.xxs),
                  itemBuilder: (context, index) {
                    final path = paths[index];
                    final preview = detail.previewItems
                        .where((item) => item.path == path)
                        .firstOrNull;
                    final change = detail.fileChanges
                        .where((item) => item.path == path)
                        .firstOrNull;
                    final subtitle = preview != null
                        ? '${preview.action}'
                              '${preview.conflict ? ' · conflict' : ''}'
                        : (change?.changeKind ?? '');
                    return CheckboxListTile(
                      contentPadding: EdgeInsets.zero,
                      dense: true,
                      value: controller.selectedPaths.contains(path),
                      onChanged: controller.mutating
                          ? null
                          : (_) => controller.togglePath(path),
                      title: Text(path, maxLines: 2),
                      subtitle: subtitle.isEmpty ? null : Text(subtitle),
                      secondary: preview?.conflict == true
                          ? Icon(
                              Icons.warning_amber,
                              color: theme.colorScheme.error,
                            )
                          : const Icon(Icons.insert_drive_file_outlined),
                    );
                  },
                ),
        ),
      ],
    );
  }
}
