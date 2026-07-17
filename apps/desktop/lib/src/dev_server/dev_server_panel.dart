import 'package:flutter/material.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

import 'dev_server_controller.dart';
import 'dev_server_models.dart';

class TaskDevServerSection extends StatelessWidget {
  const TaskDevServerSection({
    required this.controller,
    required this.taskTitle,
    super.key,
  });

  final DevServerController controller;
  final String taskTitle;

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: controller,
    builder: (context, _) {
      final config = controller.config;
      return RetconPanel(
        key: const Key('task-dev-server-section'),
        label: 'Dev server preview',
        recessed: true,
        padding: const EdgeInsets.all(RetconSpacing.sm),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Row(
              children: [
                Expanded(
                  child: Text(
                    config == null
                        ? 'Dev server'
                        : '${config.framework} · ${config.url}',
                    style: Theme.of(context).textTheme.titleMedium,
                  ),
                ),
                RetconBadge(
                  label: _statusLabel(controller.status),
                  status: _retconStatus(controller.status),
                ),
              ],
            ),
            if (config?.requiredForPreview == true) ...[
              const SizedBox(height: RetconSpacing.xs),
              const Text(
                'Preview readiness is required for inspection, but it is not verification evidence and does not pass a gate.',
                key: Key('server-not-verification-evidence'),
              ),
            ],
            if (controller.snapshot?.message case final message?) ...[
              const SizedBox(height: RetconSpacing.xs),
              Text(
                message,
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            ],
            const SizedBox(height: RetconSpacing.sm),
            Wrap(
              spacing: RetconSpacing.xs,
              runSpacing: RetconSpacing.xs,
              children: [
                FilledButton.icon(
                  key: const Key('task-open-preview'),
                  onPressed: controller.loaded && !controller.transitioning
                      ? controller.startAndOpenPreview
                      : null,
                  icon: Icon(
                    controller.running
                        ? Icons.open_in_browser
                        : Icons.rocket_launch,
                  ),
                  label: Text(
                    controller.running ? 'Open preview' : 'Start & preview',
                  ),
                ),
                OutlinedButton.icon(
                  key: const Key('open-server-center'),
                  onPressed: () => DevServerDialog.show(
                    context,
                    controller: controller,
                    title: taskTitle,
                  ),
                  icon: const Icon(Icons.dns_outlined),
                  label: const Text('Server center'),
                ),
              ],
            ),
          ],
        ),
      );
    },
  );
}

class DevServerDialog extends StatelessWidget {
  const DevServerDialog({
    required this.controller,
    required this.title,
    super.key,
  });

  final DevServerController controller;
  final String title;

  static Future<void> show(
    BuildContext context, {
    required DevServerController controller,
    required String title,
  }) => showDialog<void>(
    context: context,
    builder: (context) => DevServerDialog(controller: controller, title: title),
  );

  @override
  Widget build(BuildContext context) => Dialog(
    insetPadding: const EdgeInsets.all(RetconSpacing.md),
    child: SizedBox(
      width: 1040,
      height: 700,
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
                    'Dev server · $title',
                    style: Theme.of(context).textTheme.titleLarge,
                  ),
                ),
                IconButton(
                  tooltip: 'Close server center',
                  onPressed: () => Navigator.of(context).pop(),
                  icon: const Icon(Icons.close),
                ),
              ],
            ),
          ),
          Expanded(child: DevServerPanel(controller: controller)),
        ],
      ),
    ),
  );
}

class DevServerPanel extends StatelessWidget {
  const DevServerPanel({required this.controller, super.key});

  final DevServerController controller;

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: controller,
    builder: (context, _) {
      if (controller.loading && !controller.loaded) {
        return const Center(child: CircularProgressIndicator());
      }
      if (controller.config == null) {
        return Center(
          child: Text(controller.error ?? 'No project is available.'),
        );
      }
      return DefaultTabController(
        length: 3,
        child: Column(
          children: [
            _ServerHeader(controller: controller),
            const TabBar(
              tabs: [
                Tab(icon: Icon(Icons.dashboard_outlined), text: 'Overview'),
                Tab(icon: Icon(Icons.terminal), text: 'Live logs'),
                Tab(icon: Icon(Icons.key), text: 'Environment'),
              ],
            ),
            Expanded(
              child: TabBarView(
                children: [
                  _Overview(controller: controller),
                  _Logs(controller: controller),
                  _Environment(controller: controller),
                ],
              ),
            ),
          ],
        ),
      );
    },
  );
}

class _ServerHeader extends StatelessWidget {
  const _ServerHeader({required this.controller});
  final DevServerController controller;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.all(RetconSpacing.md),
    child: Row(
      children: [
        RetconBadge(
          label: _statusLabel(controller.status),
          status: _retconStatus(controller.status),
        ),
        const SizedBox(width: RetconSpacing.sm),
        Expanded(
          child: Text(
            controller.url ?? 'No preview URL',
            style: Theme.of(context).textTheme.titleMedium,
          ),
        ),
        FilledButton.icon(
          key: const Key('server-start'),
          onPressed: !controller.running && !controller.transitioning
              ? controller.start
              : null,
          icon: const Icon(Icons.play_arrow),
          label: const Text('Start'),
        ),
        const SizedBox(width: RetconSpacing.xs),
        OutlinedButton.icon(
          key: const Key('server-stop'),
          onPressed: controller.running && !controller.transitioning
              ? controller.stop
              : null,
          icon: const Icon(Icons.stop),
          label: const Text('Stop'),
        ),
        const SizedBox(width: RetconSpacing.xs),
        OutlinedButton.icon(
          key: const Key('server-restart'),
          onPressed: !controller.transitioning ? controller.restart : null,
          icon: const Icon(Icons.restart_alt),
          label: const Text('Restart'),
        ),
        const SizedBox(width: RetconSpacing.xs),
        FilledButton.tonalIcon(
          key: const Key('server-open-preview'),
          onPressed: controller.canOpenPreview ? controller.openPreview : null,
          icon: const Icon(Icons.open_in_browser),
          label: const Text('Open preview'),
        ),
      ],
    ),
  );
}

class _Overview extends StatelessWidget {
  const _Overview({required this.controller});
  final DevServerController controller;

  @override
  Widget build(BuildContext context) {
    final config = controller.config!;
    final snapshot = controller.snapshot;
    return ListView(
      key: const Key('server-overview'),
      padding: const EdgeInsets.all(RetconSpacing.md),
      children: [
        if (snapshot?.message case final message?) ...[
          MaterialBanner(
            content: Text(message),
            actions: [
              if (snapshot?.status == DevServerStatus.portConflict &&
                  snapshot?.suggestedPort != null)
                TextButton(
                  key: const Key('server-use-alternate-port'),
                  onPressed: controller.useSuggestedPort,
                  child: Text('Use port ${snapshot!.suggestedPort}'),
                ),
              TextButton(
                onPressed: controller.restart,
                child: const Text('Retry'),
              ),
            ],
          ),
          const SizedBox(height: RetconSpacing.sm),
        ],
        RetconPanel(
          label: 'Detected project server',
          recessed: true,
          child: Column(
            children: [
              _Field(label: 'Framework', value: config.framework),
              _Field(label: 'Status', value: _statusLabel(controller.status)),
              _Field(label: 'URL', value: config.url),
              _Field(label: 'Port', value: config.port.toString()),
              _Field(label: 'Worktree', value: config.worktreePath),
              _Field(label: 'Startup command', value: config.startupCommand),
            ],
          ),
        ),
        const SizedBox(height: RetconSpacing.sm),
        Wrap(
          spacing: RetconSpacing.xs,
          runSpacing: RetconSpacing.xs,
          children: [
            OutlinedButton.icon(
              key: const Key('server-edit-command'),
              onPressed: () async {
                final command = await _prompt(
                  context,
                  title: 'Startup command',
                  label: 'Command',
                  initialValue: config.startupCommand,
                );
                if (command != null) {
                  await controller.updateStartupCommand(command);
                }
              },
              icon: const Icon(Icons.edit),
              label: const Text('Edit command'),
            ),
            OutlinedButton.icon(
              key: const Key('server-change-port'),
              onPressed: () async {
                final value = await _prompt(
                  context,
                  title: 'Change preview port',
                  label: 'Port',
                  initialValue: config.port.toString(),
                );
                final port = int.tryParse(value ?? '');
                if (port != null) {
                  await controller.changePort(port, restartIfRunning: true);
                }
              },
              icon: const Icon(Icons.settings_ethernet),
              label: const Text('Change port'),
            ),
          ],
        ),
        SwitchListTile.adaptive(
          key: const Key('server-auto-start'),
          value: config.autoStart,
          onChanged: controller.setAutoStart,
          title: const Text('Auto-start for this project'),
          subtitle: const Text(
            'Saved and applied when this project is opened.',
          ),
        ),
        SwitchListTile.adaptive(
          key: const Key('server-required-preview'),
          value: config.requiredForPreview,
          onChanged: controller.setRequiredForPreview,
          title: const Text('Required for preview inspection'),
          subtitle: const Text(
            'Readiness remains separate from verification success.',
          ),
        ),
      ],
    );
  }
}

class _Logs extends StatelessWidget {
  const _Logs({required this.controller});
  final DevServerController controller;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.all(RetconSpacing.md),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Text(
              'Bounded live output',
              style: Theme.of(context).textTheme.titleMedium,
            ),
            const Spacer(),
            TextButton.icon(
              onPressed: controller.clearLogs,
              icon: const Icon(Icons.clear_all),
              label: const Text('Clear'),
            ),
          ],
        ),
        const SizedBox(height: RetconSpacing.xs),
        Expanded(
          child: Row(
            children: [
              Expanded(
                child: _LogPane(
                  key: const Key('server-stdout'),
                  label: 'stdout',
                  value: controller.stdout,
                ),
              ),
              const SizedBox(width: RetconSpacing.sm),
              Expanded(
                child: _LogPane(
                  key: const Key('server-stderr'),
                  label: 'stderr',
                  value: controller.stderr,
                ),
              ),
            ],
          ),
        ),
      ],
    ),
  );
}

class _LogPane extends StatelessWidget {
  const _LogPane({required this.label, required this.value, super.key});
  final String label;
  final String value;

  @override
  Widget build(BuildContext context) => RetconPanel(
    label: label,
    recessed: true,
    child: SingleChildScrollView(
      child: SelectableText(
        value.isEmpty ? 'No output yet.' : value,
        style: Theme.of(
          context,
        ).textTheme.bodySmall?.copyWith(fontFamily: 'monospace'),
      ),
    ),
  );
}

class _Environment extends StatelessWidget {
  const _Environment({required this.controller});
  final DevServerController controller;

  @override
  Widget build(BuildContext context) {
    final variables = controller.config!.environment;
    return ListView(
      key: const Key('server-environment'),
      padding: const EdgeInsets.all(RetconSpacing.md),
      children: [
        Row(
          children: [
            Expanded(
              child: Text(
                'Project environment',
                style: Theme.of(context).textTheme.titleMedium,
              ),
            ),
            FilledButton.icon(
              key: const Key('server-add-environment'),
              onPressed: () async {
                final value = await _editEnvironment(context);
                if (value != null) {
                  await controller.saveEnvironment([...variables, value]);
                }
              },
              icon: const Icon(Icons.add),
              label: const Text('Add variable'),
            ),
          ],
        ),
        const SizedBox(height: RetconSpacing.sm),
        if (variables.isEmpty)
          const Text('No environment variables configured.'),
        for (var index = 0; index < variables.length; index++)
          ListTile(
            key: Key('server-env-${variables[index].key}'),
            leading: Icon(
              variables[index].secret ? Icons.key : Icons.data_object,
            ),
            title: Text(variables[index].key),
            subtitle: Text(
              variables[index].secret ? '••••••••' : variables[index].value,
            ),
            trailing: Wrap(
              children: [
                IconButton(
                  tooltip: 'Edit variable',
                  onPressed: () async {
                    final value = await _editEnvironment(
                      context,
                      initial: variables[index],
                    );
                    if (value == null) return;
                    final next = [...variables]..[index] = value;
                    await controller.saveEnvironment(next);
                  },
                  icon: const Icon(Icons.edit),
                ),
                IconButton(
                  tooltip: 'Delete variable',
                  onPressed: () async {
                    final next = [...variables]..removeAt(index);
                    await controller.saveEnvironment(next);
                  },
                  icon: const Icon(Icons.delete_outline),
                ),
              ],
            ),
          ),
      ],
    );
  }
}

class _Field extends StatelessWidget {
  const _Field({required this.label, required this.value});
  final String label;
  final String value;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.symmetric(vertical: RetconSpacing.xs),
    child: Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SizedBox(
          width: 150,
          child: Text(label, style: Theme.of(context).textTheme.labelMedium),
        ),
        Expanded(child: SelectableText(value)),
      ],
    ),
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

Future<DevServerEnvironmentVariable?> _editEnvironment(
  BuildContext context, {
  DevServerEnvironmentVariable? initial,
}) {
  var key = initial?.key ?? '';
  var value = initial?.value ?? '';
  var secret = initial?.secret ?? false;
  return showDialog<DevServerEnvironmentVariable>(
    context: context,
    builder: (context) => StatefulBuilder(
      builder: (context, setState) => AlertDialog(
        title: Text(
          initial == null ? 'Add environment variable' : 'Edit variable',
        ),
        content: SizedBox(
          width: 420,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              TextFormField(
                initialValue: key,
                autofocus: true,
                decoration: const InputDecoration(labelText: 'Name'),
                onChanged: (next) => key = next,
              ),
              const SizedBox(height: RetconSpacing.sm),
              TextFormField(
                initialValue: value,
                obscureText: secret,
                decoration: const InputDecoration(labelText: 'Value'),
                onChanged: (next) => value = next,
              ),
              CheckboxListTile(
                value: secret,
                onChanged: (next) => setState(() => secret = next ?? false),
                title: const Text('Secret value'),
                subtitle: const Text('Masked everywhere in the server UI.'),
                contentPadding: EdgeInsets.zero,
              ),
            ],
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () {
              if (key.trim().isEmpty) return;
              Navigator.of(context).pop(
                DevServerEnvironmentVariable(
                  key: key.trim(),
                  value: value,
                  secret: secret,
                ),
              );
            },
            child: const Text('Save'),
          ),
        ],
      ),
    ),
  );
}

String _statusLabel(DevServerStatus status) => switch (status) {
  DevServerStatus.stopped => 'Stopped',
  DevServerStatus.starting => 'Starting',
  DevServerStatus.running => 'Running',
  DevServerStatus.stopping => 'Stopping',
  DevServerStatus.crashed => 'Crashed',
  DevServerStatus.startupFailed => 'Startup failed',
  DevServerStatus.portConflict => 'Port conflict',
};

RetconStatus _retconStatus(DevServerStatus status) => switch (status) {
  DevServerStatus.running => RetconStatus.success,
  DevServerStatus.starting ||
  DevServerStatus.stopping ||
  DevServerStatus.portConflict => RetconStatus.warning,
  DevServerStatus.crashed ||
  DevServerStatus.startupFailed => RetconStatus.error,
  DevServerStatus.stopped => RetconStatus.neutral,
};
