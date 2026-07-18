import 'dart:async';

import 'package:flutter/material.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

import 'diagnostics_controller.dart';
import 'diagnostics_models.dart';

class DiagnosticsDialog extends StatefulWidget {
  const DiagnosticsDialog({super.key, this.controller});

  final DiagnosticsController? controller;

  @override
  State<DiagnosticsDialog> createState() => _DiagnosticsDialogState();
}

class _DiagnosticsDialogState extends State<DiagnosticsDialog> {
  @override
  void initState() {
    super.initState();
    if (widget.controller?.snapshot == null) {
      unawaited(widget.controller?.load());
    }
  }

  @override
  Widget build(BuildContext context) => Dialog(
    insetPadding: const EdgeInsets.all(RetconSpacing.md),
    child: ConstrainedBox(
      constraints: const BoxConstraints(maxWidth: 980, maxHeight: 760),
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
                const Icon(Icons.monitor_heart),
                const SizedBox(width: RetconSpacing.sm),
                Text(
                  'Diagnostics',
                  style: Theme.of(context).textTheme.titleLarge,
                ),
                const Spacer(),
                IconButton(
                  tooltip: 'Close diagnostics',
                  onPressed: () => Navigator.of(context).pop(),
                  icon: const Icon(Icons.close),
                ),
              ],
            ),
          ),
          Expanded(
            child: widget.controller == null
                ? const _UnavailableDiagnostics()
                : DiagnosticsPanel(controller: widget.controller!),
          ),
        ],
      ),
    ),
  );
}

class DiagnosticsPanel extends StatelessWidget {
  const DiagnosticsPanel({required this.controller, super.key});

  final DiagnosticsController controller;

  @override
  Widget build(BuildContext context) => DefaultTabController(
    length: 6,
    child: AnimatedBuilder(
      animation: controller,
      builder: (context, _) => Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Semantics(
            label: 'Diagnostics sections',
            child: const TabBar(
              isScrollable: true,
              tabs: [
                Tab(icon: Icon(Icons.dashboard), text: 'Overview'),
                Tab(icon: Icon(Icons.speed), text: 'Performance'),
                Tab(icon: Icon(Icons.error_outline), text: 'Recent Errors'),
                Tab(icon: Icon(Icons.hub), text: 'Owned Resources'),
                Tab(icon: Icon(Icons.privacy_tip), text: 'Privacy & Data'),
                Tab(icon: Icon(Icons.support_agent), text: 'Support Bundle'),
              ],
            ),
          ),
          if (controller.error case final error?)
            MaterialBanner(
              content: Text(
                'Diagnostics could not complete the request: $error',
                key: const Key('diagnostics-error'),
              ),
              actions: [
                TextButton(
                  onPressed: controller.load,
                  child: const Text('Retry'),
                ),
              ],
            ),
          Expanded(child: _body()),
        ],
      ),
    ),
  );

  Widget _body() {
    if (controller.loading && controller.snapshot == null) {
      return const Center(child: CircularProgressIndicator());
    }
    final snapshot = controller.snapshot;
    if (snapshot == null) {
      return _EmptyDiagnostics(onRetry: controller.load);
    }
    return TabBarView(
      children: [
        _Overview(snapshot: snapshot),
        _Performance(snapshot: snapshot),
        _RecentErrors(errors: snapshot.errors),
        _OwnedResources(snapshot: snapshot),
        _PrivacyAndData(controller: controller, privacy: snapshot.privacy),
        _SupportBundle(controller: controller),
      ],
    );
  }
}

class _UnavailableDiagnostics extends StatelessWidget {
  const _UnavailableDiagnostics();

  @override
  Widget build(BuildContext context) => Center(
    child: Semantics(
      liveRegion: true,
      child: const Padding(
        padding: EdgeInsets.all(RetconSpacing.lg),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(Icons.cloud_off, size: 40),
            SizedBox(height: RetconSpacing.sm),
            Text('Retcon Core is unavailable.'),
            Text('Reconnect Core to inspect local diagnostics.'),
          ],
        ),
      ),
    ),
  );
}

class _EmptyDiagnostics extends StatelessWidget {
  const _EmptyDiagnostics({required this.onRetry});
  final Future<void> Function() onRetry;

  @override
  Widget build(BuildContext context) => Center(
    child: Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        const Text('No diagnostic snapshot is available.'),
        const SizedBox(height: RetconSpacing.sm),
        OutlinedButton.icon(
          onPressed: onRetry,
          icon: const Icon(Icons.refresh),
          label: const Text('Retry'),
        ),
      ],
    ),
  );
}

class _Overview extends StatelessWidget {
  const _Overview({required this.snapshot});
  final DiagnosticsSnapshot snapshot;

  @override
  Widget build(BuildContext context) => ListView(
    key: const Key('diagnostics-overview'),
    padding: const EdgeInsets.all(RetconSpacing.md),
    children: [
      Wrap(
        spacing: RetconSpacing.sm,
        runSpacing: RetconSpacing.sm,
        children: [
          _SummaryCard(
            label: 'Core',
            value: snapshot.overview.coreStatus,
            icon: Icons.memory,
          ),
          _SummaryCard(
            label: 'Version',
            value: snapshot.overview.version,
            icon: Icons.info_outline,
          ),
          _SummaryCard(
            label: 'Uptime',
            value: _duration(snapshot.overview.uptime),
            icon: Icons.schedule,
          ),
          _SummaryCard(
            label: 'Diagnostic storage',
            value: _bytes(snapshot.overview.storageBytes),
            icon: Icons.storage,
          ),
        ],
      ),
      const SizedBox(height: RetconSpacing.md),
      Text('At a glance', style: Theme.of(context).textTheme.titleMedium),
      const SizedBox(height: RetconSpacing.xs),
      Text('${snapshot.errors.length} recent errors retained within bounds.'),
      Text(
        '${snapshot.processes.length} processes, '
        '${snapshot.sessions.length} sessions, and '
        '${snapshot.ports.length} ports owned by Retcon.',
      ),
      Text(
        snapshot.privacy.telemetryEnabled
            ? 'Optional performance telemetry is enabled.'
            : 'Optional performance telemetry is off.',
      ),
    ],
  );
}

class _SummaryCard extends StatelessWidget {
  const _SummaryCard({
    required this.label,
    required this.value,
    required this.icon,
  });
  final String label;
  final String value;
  final IconData icon;

  @override
  Widget build(BuildContext context) => SizedBox(
    width: 205,
    child: Card(
      child: Padding(
        padding: const EdgeInsets.all(RetconSpacing.sm),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Icon(icon),
            const SizedBox(height: RetconSpacing.xs),
            Text(label, style: Theme.of(context).textTheme.labelMedium),
            Text(value, style: Theme.of(context).textTheme.titleMedium),
          ],
        ),
      ),
    ),
  );
}

class _Performance extends StatelessWidget {
  const _Performance({required this.snapshot});
  final DiagnosticsSnapshot snapshot;

  @override
  Widget build(BuildContext context) => ListView(
    key: const Key('diagnostics-performance'),
    padding: const EdgeInsets.all(RetconSpacing.md),
    children: [
      _DistributionCard(label: 'Core IPC roundtrips', value: snapshot.ipc),
      _DistributionCard(
        label: 'Flutter UI frames',
        value: snapshot.uiFrames,
        note: snapshot.privacy.telemetryEnabled
            ? '${snapshot.uiFrames.jankCount} frames exceeded 16 ms.'
            : 'Frame timing collection is disabled until telemetry is enabled.',
      ),
    ],
  );
}

class _DistributionCard extends StatelessWidget {
  const _DistributionCard({
    required this.label,
    required this.value,
    this.note,
  });
  final String label;
  final DiagnosticDistribution value;
  final String? note;

  @override
  Widget build(BuildContext context) => Card(
    child: Padding(
      padding: const EdgeInsets.all(RetconSpacing.md),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(label, style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: RetconSpacing.sm),
          Wrap(
            spacing: RetconSpacing.sm,
            runSpacing: RetconSpacing.xs,
            children: [
              RetconBadge(label: '${value.count} samples'),
              RetconBadge(label: 'p50 ${value.p50Ms.toStringAsFixed(1)} ms'),
              RetconBadge(label: 'p95 ${value.p95Ms.toStringAsFixed(1)} ms'),
              RetconBadge(label: 'max ${value.maxMs.toStringAsFixed(1)} ms'),
            ],
          ),
          if (note != null) ...[
            const SizedBox(height: RetconSpacing.xs),
            Text(note!),
          ],
        ],
      ),
    ),
  );
}

class _RecentErrors extends StatelessWidget {
  const _RecentErrors({required this.errors});
  final List<DiagnosticError> errors;

  @override
  Widget build(BuildContext context) {
    if (errors.isEmpty) {
      return const Center(child: Text('No recent diagnostic errors.'));
    }
    return ListView.separated(
      key: const Key('diagnostics-recent-errors'),
      padding: const EdgeInsets.all(RetconSpacing.md),
      itemCount: errors.length,
      separatorBuilder: (_, _) => const SizedBox(height: RetconSpacing.xs),
      itemBuilder: (context, index) {
        final error = errors[index];
        return Semantics(
          label: '${error.severity.name} ${error.component} ${error.code}',
          child: Card(
            child: ListTile(
              leading: Icon(
                error.severity == DiagnosticSeverity.critical
                    ? Icons.dangerous
                    : Icons.error_outline,
                color: Theme.of(context).colorScheme.error,
              ),
              title: Text('${error.component} · ${error.code}'),
              subtitle: Text('${error.message}\n${_time(error.timestamp)}'),
              isThreeLine: true,
            ),
          ),
        );
      },
    );
  }
}

class _OwnedResources extends StatelessWidget {
  const _OwnedResources({required this.snapshot});
  final DiagnosticsSnapshot snapshot;

  @override
  Widget build(BuildContext context) => ListView(
    key: const Key('diagnostics-owned-resources'),
    padding: const EdgeInsets.all(RetconSpacing.md),
    children: [
      _ResourceSection(label: 'Processes', values: snapshot.processes),
      _ResourceSection(label: 'Sessions', values: snapshot.sessions),
      _ResourceSection(label: 'Ports', values: snapshot.ports),
    ],
  );
}

class _ResourceSection extends StatelessWidget {
  const _ResourceSection({required this.label, required this.values});
  final String label;
  final List<DiagnosticOwnedResource> values;

  @override
  Widget build(BuildContext context) => Card(
    child: ExpansionTile(
      initiallyExpanded: true,
      title: Text('$label (${values.length})'),
      children: values.isEmpty
          ? const [ListTile(title: Text('None owned by Retcon.'))]
          : [
              for (final value in values)
                ListTile(
                  title: Text(
                    value.port == null
                        ? value.kind
                        : '${value.kind} · port ${value.port}',
                  ),
                  subtitle: Text(
                    '${value.status} · ${value.id}'
                    '${value.startedAt == null ? '' : ' · ${_time(value.startedAt!)}'}',
                  ),
                ),
            ],
    ),
  );
}

class _PrivacyAndData extends StatelessWidget {
  const _PrivacyAndData({required this.controller, required this.privacy});
  final DiagnosticsController controller;
  final DiagnosticPrivacy privacy;

  @override
  Widget build(BuildContext context) => ListView(
    key: const Key('diagnostics-privacy'),
    padding: const EdgeInsets.all(RetconSpacing.md),
    children: [
      SwitchListTile(
        key: const Key('diagnostics-telemetry-toggle'),
        contentPadding: EdgeInsets.zero,
        value: privacy.telemetryEnabled,
        onChanged: controller.updatingPrivacy ? null : controller.setTelemetry,
        title: const Text('Optional performance telemetry'),
        subtitle: const Text(
          'Off by default. When enabled, Retcon records bounded IPC latency '
          'and UI frame timing locally. No network exporter is used.',
        ),
      ),
      const Divider(),
      Text('Never collected', style: Theme.of(context).textTheme.titleMedium),
      const Text(
        'Prompts, terminal output or commands, filesystem paths, file contents, '
        'environment values, cookies, headers, credentials, and secrets are '
        'excluded from desktop diagnostic records.',
      ),
      const SizedBox(height: RetconSpacing.md),
      Text('Recorded fields', style: Theme.of(context).textTheme.titleMedium),
      Text('Retention: ${privacy.retentionDays} days'),
      if (privacy.fields.isEmpty)
        const Text('Core did not report any retained diagnostic fields.'),
      for (final field in privacy.fields)
        ListTile(
          title: Text(field.name),
          subtitle: Text('${field.purpose}\nRetention: ${field.retention}'),
          isThreeLine: true,
        ),
      const Divider(),
      Align(
        alignment: Alignment.centerLeft,
        child: OutlinedButton.icon(
          key: const Key('delete-diagnostic-data'),
          onPressed: controller.deleting
              ? null
              : () => _confirmDelete(context, controller),
          icon: const Icon(Icons.delete_outline),
          label: const Text('Delete diagnostic data'),
        ),
      ),
    ],
  );
}

class _SupportBundle extends StatelessWidget {
  const _SupportBundle({required this.controller});
  final DiagnosticsController controller;

  @override
  Widget build(BuildContext context) => ListView(
    key: const Key('diagnostics-support-bundle'),
    padding: const EdgeInsets.all(RetconSpacing.md),
    children: [
      Text(
        'Sanitized support bundle',
        style: Theme.of(context).textTheme.titleMedium,
      ),
      const SizedBox(height: RetconSpacing.xs),
      const Text(
        'Core creates the bundle from allowlisted local diagnostics. The '
        'desktop receives only a bundle identifier, safe file name, size, and '
        'creation time—never an absolute local path.',
      ),
      const SizedBox(height: RetconSpacing.md),
      Align(
        alignment: Alignment.centerLeft,
        child: FilledButton.icon(
          key: const Key('export-support-bundle'),
          onPressed: controller.exporting
              ? null
              : () async {
                  await controller.exportSupportBundle();
                  if (!context.mounted) return;
                  final bundle = controller.lastBundle;
                  if (bundle != null) {
                    ScaffoldMessenger.of(context).showSnackBar(
                      SnackBar(content: Text('Created ${bundle.fileName}')),
                    );
                  }
                },
          icon: const Icon(Icons.archive_outlined),
          label: Text(controller.exporting ? 'Creating…' : 'Create bundle'),
        ),
      ),
      if (controller.lastBundle case final bundle?) ...[
        const SizedBox(height: RetconSpacing.md),
        Card(
          child: ListTile(
            leading: const Icon(Icons.verified_user),
            title: Text(bundle.fileName),
            subtitle: Text(
              '${_bytes(bundle.sizeBytes)} · ${_time(bundle.createdAt)}\n'
              'Bundle ID: ${bundle.id}',
            ),
            isThreeLine: true,
          ),
        ),
      ],
    ],
  );
}

Future<void> _confirmDelete(
  BuildContext context,
  DiagnosticsController controller,
) async {
  final confirmed = await showDialog<bool>(
    context: context,
    builder: (context) => AlertDialog(
      title: const Text('Delete diagnostic data?'),
      content: const Text(
        'This permanently removes retained logs, metrics, recent errors, and '
        'support bundles. Project files and task data are not affected.',
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(false),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const Key('confirm-delete-diagnostic-data'),
          onPressed: () => Navigator.of(context).pop(true),
          child: const Text('Delete'),
        ),
      ],
    ),
  );
  if (confirmed != true) return;
  final deleted = await controller.deleteDiagnosticData();
  if (!context.mounted || deleted == null) return;
  final total = deleted.values.fold<int>(0, (sum, value) => sum + value);
  ScaffoldMessenger.of(
    context,
  ).showSnackBar(SnackBar(content: Text('Deleted $total diagnostic records.')));
}

String _duration(Duration value) {
  final hours = value.inHours;
  final minutes = value.inMinutes.remainder(60);
  return hours > 0 ? '${hours}h ${minutes}m' : '${minutes}m';
}

String _bytes(int value) {
  if (value >= 1024 * 1024) {
    return '${(value / (1024 * 1024)).toStringAsFixed(1)} MiB';
  }
  if (value >= 1024) return '${(value / 1024).toStringAsFixed(1)} KiB';
  return '$value B';
}

String _time(DateTime value) => value.toLocal().toIso8601String();
