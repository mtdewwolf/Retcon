import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

import 'core_client.dart';
import 'diagnostics/core_diagnostics_repository.dart';
import 'diagnostics/desktop_diagnostics.dart';

/// Provider setup checks with repair actions and diagnostic export.
class ProviderDoctorDialog extends StatefulWidget {
  const ProviderDoctorDialog({super.key, required this.core});
  final CoreClient? core;

  @override
  State<ProviderDoctorDialog> createState() => _ProviderDoctorDialogState();
}

class _ProviderDoctorDialogState extends State<ProviderDoctorDialog> {
  Map<String, dynamic>? _report;
  String? _error;
  bool _loading = true;
  String? _exportName;
  List<Map<String, dynamic>>? _pathHints;

  @override
  void initState() {
    super.initState();
    unawaited(_refresh(forceRefresh: true));
  }

  Future<void> _refresh({bool forceRefresh = false}) async {
    setState(() {
      _loading = true;
      _error = null;
      _exportName = null;
      _pathHints = null;
    });
    try {
      final result =
          await widget.core?.request(
            'provider.doctor',
            params: {'forceRefresh': forceRefresh},
          ) ??
          <String, dynamic>{};
      if (mounted) setState(() => _report = result);
    } on Object {
      DesktopDiagnostics.instance.captureOperationFailure(
        component: 'desktop.provider_doctor',
        code: 'provider_doctor_load_failed',
      );
      if (mounted) {
        setState(() => _error = 'Provider checks are temporarily unavailable.');
      }
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  Future<void> _exportDiagnostics() async {
    final core = widget.core;
    if (core == null || core.status != CoreConnectionStatus.connected) {
      setState(() => _error = 'Retcon Core is unavailable.');
      return;
    }
    try {
      final bundle = await CoreDiagnosticsRepository.fromCore(
        core,
      ).exportSupportBundle(context: 'provider_doctor');
      if (!mounted) return;
      setState(() => _exportName = bundle.fileName);
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Created sanitized bundle ${bundle.fileName}')),
      );
    } on Object {
      DesktopDiagnostics.instance.captureOperationFailure(
        component: 'desktop.provider_doctor',
        code: 'provider_doctor_bundle_failed',
      );
      if (mounted) {
        setState(() => _error = 'Could not create the support bundle.');
      }
    }
  }

  Future<void> _runRepair(Map<String, dynamic> action) async {
    switch (action['kind']) {
      case 'open_docs':
        final url = action['url']?.toString();
        if (url != null && url.isNotEmpty) {
          await _openUrl(url);
        }
      case 'reveal_config':
        final path = action['path']?.toString();
        if (path != null && path.isNotEmpty) {
          await _revealPath(path);
        }
      case 'path_hints':
        final hints = (action['hints'] as List? ?? const [])
            .cast<Map>()
            .map((entry) => entry.cast<String, dynamic>())
            .toList();
        setState(() => _pathHints = hints);
      case 'retry':
        await _refresh(forceRefresh: true);
      default:
        break;
    }
  }

  Future<void> _openUrl(String url) async {
    if (Platform.isWindows) {
      await Process.run('cmd', ['/c', 'start', '', url]);
      return;
    }
    if (Platform.isMacOS) {
      await Process.run('open', [url]);
      return;
    }
    await Process.run('xdg-open', [url]);
  }

  Future<void> _revealPath(String path) async {
    final target = Directory(path).existsSync() ? path : File(path).parent.path;
    if (Platform.isWindows) {
      await Process.run('explorer', [target]);
      return;
    }
    if (Platform.isMacOS) {
      await Process.run('open', [target]);
      return;
    }
    await Process.run('xdg-open', [target]);
  }

  RetconStatus _overallStatus() {
    final status = _report?['overall_status']?.toString() ?? 'failure';
    return switch (status) {
      'ready' => RetconStatus.success,
      'warning' => RetconStatus.warning,
      _ => RetconStatus.error,
    };
  }

  String _overallLabel() {
    final status = _report?['overall_status']?.toString() ?? 'failure';
    return switch (status) {
      'ready' => 'Healthy',
      'warning' => 'Needs attention',
      _ => 'Blocked',
    };
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('Provider Doctor'),
    content: SizedBox(
      width: 680,
      child: _loading
          ? const Center(child: CircularProgressIndicator())
          : _error != null
          ? Text('Could not run the provider checks: $_error')
          : _report == null || _report!.isEmpty
          ? const Text('Retcon Core is unavailable. Start the core and retry.')
          : _DoctorReportView(
              report: _report!,
              pathHints: _pathHints,
              onRepair: _runRepair,
            ),
    ),
    actions: [
      if (_report != null && _report!.isNotEmpty) ...[
        RetconBadge(label: _overallLabel(), status: _overallStatus()),
        const SizedBox(width: 8),
        TextButton(
          onPressed: _exportDiagnostics,
          child: const Text('Export bundle'),
        ),
        if (_exportName != null) Text('Created $_exportName'),
        ..._repairButtons(
          (_report!['repair_actions'] as List? ?? const []).cast<Map>(),
        ),
      ],
      TextButton(
        onPressed: _loading ? null : () => _refresh(forceRefresh: true),
        child: const Text('Retry checks'),
      ),
      FilledButton(
        onPressed: () => Navigator.of(context).pop(),
        child: const Text('Close'),
      ),
    ],
  );

  List<Widget> _repairButtons(List<Map> actions) => [
    for (final raw in actions)
      TextButton(
        onPressed: () => _runRepair(raw.cast<String, dynamic>()),
        child: Text(raw['label']?.toString() ?? 'Repair'),
      ),
  ];
}

class _DoctorReportView extends StatelessWidget {
  const _DoctorReportView({
    required this.report,
    required this.pathHints,
    required this.onRepair,
  });

  final Map<String, dynamic> report;
  final List<Map<String, dynamic>>? pathHints;
  final Future<void> Function(Map<String, dynamic> action) onRepair;

  @override
  Widget build(BuildContext context) {
    final checks = (report['checks'] as List? ?? const []).cast<Map>();
    final hints =
        pathHints ?? (report['path_hints'] as List? ?? const []).cast<Map>();
    return SingleChildScrollView(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            '${report['provider_name'] ?? 'Provider'} ${report['version'] ?? 'not installed'}',
            style: Theme.of(context).textTheme.titleMedium,
          ),
          const SizedBox(height: 8),
          Text(
            'Minimum supported version: ${report['minimum_supported_version'] ?? 'unknown'}',
            style: Theme.of(context).textTheme.bodySmall,
          ),
          const SizedBox(height: 12),
          for (final raw in checks)
            _DoctorCheckTile(
              check: raw.cast<String, dynamic>(),
              onRepair: onRepair,
            ),
          if (hints.isNotEmpty) ...[
            const SizedBox(height: 12),
            Text('PATH hints', style: Theme.of(context).textTheme.titleSmall),
            const SizedBox(height: 8),
            for (final raw in hints)
              _PathHintRow(hint: raw.cast<String, dynamic>()),
          ],
        ],
      ),
    );
  }
}

class _DoctorCheckTile extends StatelessWidget {
  const _DoctorCheckTile({required this.check, required this.onRepair});
  final Map<String, dynamic> check;
  final Future<void> Function(Map<String, dynamic> action) onRepair;

  @override
  Widget build(BuildContext context) {
    final status = check['status']?.toString() ?? 'warning';
    final retconStatus = switch (status) {
      'ready' => RetconStatus.success,
      'failure' => RetconStatus.error,
      _ => RetconStatus.warning,
    };
    final icon = switch (status) {
      'ready' => Icons.check_circle,
      'failure' => Icons.error,
      _ => Icons.warning,
    };
    final color = switch (retconStatus) {
      RetconStatus.success => Colors.greenAccent,
      RetconStatus.error => Colors.redAccent,
      RetconStatus.warning => Colors.amberAccent,
      RetconStatus.neutral => Theme.of(context).colorScheme.outline,
    };
    final actions = (check['repair_actions'] as List? ?? const []).cast<Map>();

    return Padding(
      padding: const EdgeInsets.only(bottom: 12),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(icon, color: color),
          const SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Expanded(
                      child: Text(
                        check['label']?.toString() ?? 'Check',
                        style: Theme.of(context).textTheme.titleSmall,
                      ),
                    ),
                    RetconBadge(
                      label: switch (status) {
                        'ready' => 'Ready',
                        'failure' => 'Failure',
                        _ => 'Warning',
                      },
                      status: retconStatus,
                    ),
                  ],
                ),
                Text(check['detail']?.toString() ?? ''),
                if (check['suggested_action'] != null)
                  Text(
                    check['suggested_action'].toString(),
                    style: TextStyle(color: color),
                  ),
                if (actions.isNotEmpty)
                  Wrap(
                    spacing: 8,
                    runSpacing: 4,
                    children: [
                      for (final raw in actions)
                        TextButton(
                          onPressed: () =>
                              onRepair(raw.cast<String, dynamic>()),
                          child: Text(raw['label']?.toString() ?? 'Repair'),
                        ),
                    ],
                  ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _PathHintRow extends StatelessWidget {
  const _PathHintRow({required this.hint});
  final Map<String, dynamic> hint;

  @override
  Widget build(BuildContext context) {
    final present = hint['present'] == true;
    return Padding(
      padding: const EdgeInsets.only(bottom: 6),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(
            present ? Icons.check_circle_outline : Icons.cancel_outlined,
            size: 16,
            color: present ? Colors.greenAccent : Colors.redAccent,
          ),
          const SizedBox(width: 8),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  hint['label']?.toString() ?? 'PATH entry',
                  style: Theme.of(context).textTheme.labelMedium,
                ),
                Text(
                  hint['path']?.toString() ?? '',
                  style: Theme.of(context).textTheme.bodySmall,
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
