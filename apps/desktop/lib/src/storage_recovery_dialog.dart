import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';

import 'core_client.dart';

/// Minimal dialog shown when local storage is damaged and core cannot start.
class StorageRecoveryDialog extends StatelessWidget {
  const StorageRecoveryDialog({
    super.key,
    required this.message,
    required this.dataDirectory,
    this.onDismiss,
  });

  final String message;
  final Directory dataDirectory;
  final VoidCallback? onDismiss;

  static Future<void> showIfNeeded(
    BuildContext context, {
    required Object error,
    required Directory dataDirectory,
  }) async {
    final text = error.toString().toLowerCase();
    if (!text.contains('damaged') &&
        !text.contains('corrupt') &&
        !text.contains('database')) {
      return;
    }
    await showDialog<void>(
      context: context,
      barrierDismissible: false,
      builder: (context) => StorageRecoveryDialog(
        message: error.toString(),
        dataDirectory: dataDirectory,
        onDismiss: () => Navigator.of(context).pop(),
      ),
    );
  }

  Future<void> _runRecovery(BuildContext context, String action) async {
    final executable = Platform.resolvedExecutable;
    final coreBinary = Platform.isWindows
        ? '${File(executable).parent.path}${Platform.pathSeparator}retcon-core.exe'
        : '${File(executable).parent.path}${Platform.pathSeparator}retcon-core';
    final result = await Process.run(coreBinary, [
      '--storage-recover',
      action,
      '--data-dir',
      dataDirectory.path,
      '--log-format',
      'json',
    ]);
    if (!context.mounted) return;
    final output = result.stdout.toString().trim();
    Map<String, dynamic>? report;
    try {
      report = jsonDecode(output) as Map<String, dynamic>;
    } on Object {
      report = null;
    }
    await showDialog<void>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Storage recovery'),
        content: Text(
          report == null
              ? (result.stderr.toString().trim().isEmpty
                    ? 'Recovery finished with exit code ${result.exitCode}.'
                    : result.stderr.toString())
              : const JsonEncoder.withIndent('  ').convert(report),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(),
            child: const Text('Close'),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('Local storage needs attention'),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(message),
          const SizedBox(height: 12),
          const Text(
            'You can inspect the database, create a backup, or reset to a fresh schema.',
          ),
        ],
      ),
      actions: [
        TextButton(onPressed: onDismiss, child: const Text('Dismiss')),
        TextButton(
          onPressed: () => _runRecovery(context, 'report'),
          child: const Text('Report'),
        ),
        TextButton(
          onPressed: () => _runRecovery(context, 'backup'),
          child: const Text('Backup'),
        ),
        FilledButton(
          onPressed: () => _runRecovery(context, 'reset'),
          child: const Text('Reset database'),
        ),
      ],
    );
  }
}

/// Observes core connection failures and surfaces the recovery dialog when needed.
class StorageRecoveryGate extends StatefulWidget {
  const StorageRecoveryGate({
    super.key,
    required this.core,
    required this.child,
  });

  final CoreClient core;
  final Widget child;

  @override
  State<StorageRecoveryGate> createState() => _StorageRecoveryGateState();
}

class _StorageRecoveryGateState extends State<StorageRecoveryGate> {
  Object? _lastError;

  @override
  void initState() {
    super.initState();
    widget.core.addListener(_handleCoreUpdate);
    WidgetsBinding.instance.addPostFrameCallback((_) => _handleCoreUpdate());
  }

  @override
  void dispose() {
    widget.core.removeListener(_handleCoreUpdate);
    super.dispose();
  }

  Future<void> _handleCoreUpdate() async {
    if (widget.core.status != CoreConnectionStatus.disconnected) return;
    final error = widget.core.lastConnectionError;
    if (error == null || identical(error, _lastError)) return;
    _lastError = error;
    if (!mounted) return;
    await StorageRecoveryDialog.showIfNeeded(
      context,
      error: error,
      dataDirectory: widget.core.dataDirectory,
    );
  }

  @override
  Widget build(BuildContext context) => widget.child;
}
