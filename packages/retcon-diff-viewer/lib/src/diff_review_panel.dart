import 'dart:async';

import 'package:flutter/material.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

import 'diff_models.dart';
import 'diff_service.dart';
import 'diff_viewer_panel.dart';

/// Workspace review panel for inspecting and staging repository changes.
class DiffReviewPanel extends StatefulWidget {
  const DiffReviewPanel({
    required this.service,
    required this.repo,
    this.sessionId,
    super.key,
  });

  final DiffService service;
  final String repo;
  final String? sessionId;

  @override
  State<DiffReviewPanel> createState() => _DiffReviewPanelState();
}

class _DiffReviewPanelState extends State<DiffReviewPanel> {
  DiffScope _scope = DiffScope.unstaged;
  DiffViewMode _viewMode = DiffViewMode.unified;
  final _commitController = TextEditingController(
    text: 'Reviewed agent changes',
  );
  bool _committing = false;

  @override
  void dispose() {
    _commitController.dispose();
    super.dispose();
  }

  Future<void> _commit() async {
    setState(() => _committing = true);
    try {
      final oid = await widget.service.commit(
        repo: widget.repo,
        message: _commitController.text.trim(),
      );
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Committed $oid')),
      );
    } catch (error) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Commit failed: $error')),
      );
    } finally {
      if (mounted) {
        setState(() => _committing = false);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.all(RetconSpacing.sm),
          child: Wrap(
            spacing: RetconSpacing.sm,
            runSpacing: RetconSpacing.xs,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              SegmentedButton<DiffScope>(
                segments: const [
                  ButtonSegment(
                    value: DiffScope.unstaged,
                    label: Text('Unstaged'),
                  ),
                  ButtonSegment(value: DiffScope.staged, label: Text('Staged')),
                  ButtonSegment(value: DiffScope.all, label: Text('All')),
                ],
                selected: {_scope},
                onSelectionChanged: (selection) {
                  setState(() => _scope = selection.first);
                },
              ),
              SegmentedButton<DiffViewMode>(
                segments: const [
                  ButtonSegment(
                    value: DiffViewMode.unified,
                    label: Text('Unified'),
                  ),
                  ButtonSegment(
                    value: DiffViewMode.sideBySide,
                    label: Text('Side by side'),
                  ),
                ],
                selected: {_viewMode},
                onSelectionChanged: (selection) {
                  setState(() => _viewMode = selection.first);
                },
              ),
              if (widget.sessionId != null)
                Chip(label: Text('Session ${widget.sessionId}')),
              SizedBox(
                width: 280,
                child: TextField(
                  controller: _commitController,
                  decoration: const InputDecoration(
                    labelText: 'Commit message',
                    isDense: true,
                  ),
                ),
              ),
              FilledButton(
                onPressed: _committing ? null : () => unawaited(_commit()),
                child: _committing
                    ? const SizedBox(
                        width: 16,
                        height: 16,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      )
                    : const Text('Commit'),
              ),
            ],
          ),
        ),
        const Divider(height: 1),
        Expanded(
          child: DiffViewerPanel(
            key: ValueKey('${_scope.name}-${_viewMode.name}'),
            service: widget.service,
            repo: widget.repo,
            scope: _scope,
            viewMode: _viewMode,
          ),
        ),
      ],
    );
  }
}
