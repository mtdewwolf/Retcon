import 'package:flutter/material.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

import 'project_controller.dart';
import 'project_health_report.dart';
import 'project_models.dart';

/// Clone a remote repository and open it in Retcon.
class ProjectCloneDialog extends StatefulWidget {
  const ProjectCloneDialog({super.key, required this.controller});

  final ProjectController controller;

  static Future<OpenProjectResult?> show(
    BuildContext context, {
    required ProjectController controller,
  }) {
    return showDialog<OpenProjectResult>(
      context: context,
      builder: (context) => ProjectCloneDialog(controller: controller),
    );
  }

  @override
  State<ProjectCloneDialog> createState() => _ProjectCloneDialogState();
}

class _ProjectCloneDialogState extends State<ProjectCloneDialog> {
  final _remoteController = TextEditingController();
  final _destinationController = TextEditingController();
  Object? _error;
  bool _working = false;

  @override
  void dispose() {
    _remoteController.dispose();
    _destinationController.dispose();
    super.dispose();
  }

  Future<void> _clone() async {
    final remote = _remoteController.text.trim();
    final destination = _destinationController.text.trim();
    if (remote.isEmpty || destination.isEmpty) {
      setState(() => _error = 'Enter a remote URL and destination folder.');
      return;
    }
    setState(() {
      _working = true;
      _error = null;
    });
    try {
      final opened = await widget.controller.clone(
        remoteUrl: remote,
        destination: destination,
      );
      if (!mounted) return;
      Navigator.of(context).pop(opened);
    } catch (error) {
      if (mounted) {
        setState(() {
          _error = error;
          _working = false;
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('Clone project'),
      content: SizedBox(
        width: 560,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            RetconTextField(
              label: 'Remote URL',
              hint: 'https://github.com/org/repo.git',
              controller: _remoteController,
              enabled: !_working,
            ),
            const SizedBox(height: RetconSpacing.sm),
            RetconTextField(
              label: 'Destination folder',
              hint: 'C:\\Projects\\repo',
              controller: _destinationController,
              enabled: !_working,
            ),
            if (_error != null) ...[
              const SizedBox(height: RetconSpacing.sm),
              Text(
                _error.toString(),
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            ],
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: _working ? null : () => Navigator.of(context).pop(),
          child: const Text('Cancel'),
        ),
        FilledButton(
          onPressed: _working ? null : _clone,
          child: _working
              ? const SizedBox(
                  width: 18,
                  height: 18,
                  child: CircularProgressIndicator(strokeWidth: 2),
                )
              : const Text('Clone and open'),
        ),
      ],
    );
  }
}

/// Shows health after a successful clone from the picker flow.
Future<void> showProjectHealthDialog(
  BuildContext context, {
  required OpenProjectResult project,
}) {
  return showDialog<void>(
    context: context,
    builder: (context) => AlertDialog(
      title: Text('${project.metadata.name} is ready'),
      content: SizedBox(
        width: 620,
        height: 420,
        child: SingleChildScrollView(
          child: ProjectHealthReport(
            health: project.health,
            analysis: project.analysis,
          ),
        ),
      ),
      actions: [
        FilledButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Continue'),
        ),
      ],
    ),
  );
}
