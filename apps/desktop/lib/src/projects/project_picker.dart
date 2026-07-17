import 'dart:async';

import 'package:flutter/material.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

import 'project_clone_dialog.dart';
import 'project_controller.dart';
import 'project_models.dart';

/// Recent and pinned project picker with open-by-path support.
class ProjectPickerDialog extends StatefulWidget {
  const ProjectPickerDialog({super.key, required this.controller});

  final ProjectController controller;

  static Future<OpenProjectResult?> show(
    BuildContext context, {
    required ProjectController controller,
  }) {
    return showDialog<OpenProjectResult>(
      context: context,
      builder: (context) => ProjectPickerDialog(controller: controller),
    );
  }

  @override
  State<ProjectPickerDialog> createState() => _ProjectPickerDialogState();
}

class _ProjectPickerDialogState extends State<ProjectPickerDialog> {
  final _pathController = TextEditingController();
  String _query = '';
  Object? _error;
  bool _opening = false;

  @override
  void initState() {
    super.initState();
    widget.controller.addListener(_handleControllerUpdate);
    unawaited(widget.controller.refresh());
  }

  @override
  void dispose() {
    widget.controller.removeListener(_handleControllerUpdate);
    _pathController.dispose();
    super.dispose();
  }

  void _handleControllerUpdate() {
    if (mounted) setState(() {});
  }

  Future<void> _openPath() async {
    final path = _pathController.text.trim();
    if (path.isEmpty) {
      setState(() => _error = 'Enter a project folder path.');
      return;
    }
    await _openExisting(path);
  }

  Future<void> _openExisting(String path) async {
    setState(() {
      _opening = true;
      _error = null;
    });
    try {
      final opened = await widget.controller.open(path);
      if (!mounted) return;
      Navigator.of(context).pop(opened);
    } catch (error) {
      if (mounted) {
        setState(() {
          _error = error;
          _opening = false;
        });
      }
    }
  }

  Future<void> _clone() async {
    final opened = await ProjectCloneDialog.show(
      context,
      controller: widget.controller,
    );
    if (!mounted || opened == null) return;
    Navigator.of(context).pop(opened);
  }

  Future<void> _togglePin(ProjectSummary project) async {
    try {
      await widget.controller.togglePinned(project);
    } catch (error) {
      if (mounted) setState(() => _error = error);
    }
  }

  Future<void> _remove(ProjectSummary project) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Remove from recent projects?'),
        content: Text(
          'This removes "${project.name}" from Retcon without deleting files on disk.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('Remove'),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    try {
      await widget.controller.remove(project);
    } catch (error) {
      if (mounted) setState(() => _error = error);
    }
  }

  List<ProjectSummary> get _filtered {
    final needle = _query.toLowerCase();
    return widget.controller.projects.where((project) {
      if (needle.isEmpty) return true;
      return project.name.toLowerCase().contains(needle) ||
          project.repositoryPath.toLowerCase().contains(needle);
    }).toList();
  }

  @override
  Widget build(BuildContext context) {
    final controller = widget.controller;
    final pinned = _filtered.where((project) => project.pinned).toList();
    final recent = _filtered.where((project) => !project.pinned).toList();

    return Dialog(
      child: SizedBox(
        width: 720,
        height: 560,
        child: RetconPanel(
          label: 'Open project',
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              RetconTextField(
                label: 'Search recent projects',
                hint: 'Filter by name or path…',
                onChanged: (value) => setState(() => _query = value),
              ),
              const SizedBox(height: RetconSpacing.sm),
              Row(
                children: [
                  Expanded(
                    child: RetconTextField(
                      label: 'Open folder',
                      hint: 'C:\\Projects\\my-app',
                      controller: _pathController,
                      enabled: !_opening,
                    ),
                  ),
                  const SizedBox(width: RetconSpacing.sm),
                  FilledButton(
                    onPressed: _opening ? null : _openPath,
                    child: _opening
                        ? const SizedBox(
                            width: 18,
                            height: 18,
                            child: CircularProgressIndicator(strokeWidth: 2),
                          )
                        : const Text('Open'),
                  ),
                ],
              ),
              const SizedBox(height: RetconSpacing.sm),
              Wrap(
                spacing: RetconSpacing.xs,
                children: [
                  OutlinedButton.icon(
                    onPressed: _opening ? null : _clone,
                    icon: const Icon(Icons.download),
                    label: const Text('Clone repository'),
                  ),
                  if (controller.loading)
                    const Padding(
                      padding: EdgeInsets.all(RetconSpacing.xs),
                      child: SizedBox(
                        width: 18,
                        height: 18,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      ),
                    ),
                ],
              ),
              if (_error != null) ...[
                const SizedBox(height: RetconSpacing.xs),
                Text(
                  _error.toString(),
                  style: TextStyle(color: Theme.of(context).colorScheme.error),
                ),
              ],
              const SizedBox(height: RetconSpacing.sm),
              Expanded(
                child: controller.loading && controller.projects.isEmpty
                    ? const Center(child: CircularProgressIndicator())
                    : ListView(
                        children: [
                          if (pinned.isNotEmpty) ...[
                            _SectionHeader(title: 'Pinned'),
                            for (final project in pinned)
                              _ProjectTile(
                                project: project,
                                onOpen: () => _openExisting(project.repositoryPath),
                                onTogglePin: () => _togglePin(project),
                                onRemove: () => _remove(project),
                              ),
                          ],
                          if (recent.isNotEmpty) ...[
                            _SectionHeader(title: 'Recent'),
                            for (final project in recent)
                              _ProjectTile(
                                project: project,
                                onOpen: () => _openExisting(project.repositoryPath),
                                onTogglePin: () => _togglePin(project),
                                onRemove: () => _remove(project),
                              ),
                          ],
                          if (pinned.isEmpty && recent.isEmpty)
                            const Padding(
                              padding: EdgeInsets.all(RetconSpacing.md),
                              child: Text(
                                'No recent projects yet. Open a folder or clone a repository to get started.',
                              ),
                            ),
                        ],
                      ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _SectionHeader extends StatelessWidget {
  const _SectionHeader({required this.title});
  final String title;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(
        top: RetconSpacing.sm,
        bottom: RetconSpacing.xs,
      ),
      child: Text(title, style: Theme.of(context).textTheme.titleSmall),
    );
  }
}

class _ProjectTile extends StatelessWidget {
  const _ProjectTile({
    required this.project,
    required this.onOpen,
    required this.onTogglePin,
    required this.onRemove,
  });

  final ProjectSummary project;
  final VoidCallback onOpen;
  final VoidCallback onTogglePin;
  final VoidCallback onRemove;

  @override
  Widget build(BuildContext context) {
    return ListTile(
      leading: Icon(project.pinned ? Icons.push_pin : Icons.folder),
      title: Text(project.name),
      subtitle: Text(
        project.repositoryPath,
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
      ),
      trailing: Wrap(
        spacing: RetconSpacing.xs,
        children: [
          IconButton(
            tooltip: project.pinned ? 'Unpin' : 'Pin',
            onPressed: onTogglePin,
            icon: Icon(project.pinned ? Icons.push_pin : Icons.push_pin_outlined),
          ),
          IconButton(
            tooltip: 'Remove from recent',
            onPressed: onRemove,
            icon: const Icon(Icons.delete_outline),
          ),
        ],
      ),
      onTap: onOpen,
    );
  }
}

/// Entry point used by the shell for open/clone flows.
Future<void> showProjectPickerFlow(
  BuildContext context, {
  required ProjectController controller,
  bool cloneFirst = false,
}) async {
  final OpenProjectResult? opened = cloneFirst
      ? await ProjectCloneDialog.show(context, controller: controller)
      : await ProjectPickerDialog.show(context, controller: controller);
  if (opened == null || !context.mounted) return;
  await showProjectHealthDialog(context, project: opened);
}
