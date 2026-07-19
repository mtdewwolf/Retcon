import 'dart:async';

import 'package:flutter/material.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

import 'core_client.dart';

/// Terminal-style view of Core launch output and desktop connection activity.
class CoreLogPanel extends StatefulWidget {
  const CoreLogPanel({required this.core, super.key});

  final CoreClient core;

  @override
  State<CoreLogPanel> createState() => _CoreLogPanelState();
}

class _CoreLogPanelState extends State<CoreLogPanel> {
  final _scrollController = ScrollController();
  late List<CoreLogEntry> _entries;
  StreamSubscription<CoreLogEntry>? _subscription;
  bool _retrying = false;

  @override
  void initState() {
    super.initState();
    _entries = List.of(widget.core.recentCoreLogs);
    _listen();
  }

  @override
  void didUpdateWidget(covariant CoreLogPanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.core == widget.core) return;
    _subscription?.cancel();
    _entries = List.of(widget.core.recentCoreLogs);
    _listen();
  }

  void _listen() {
    _subscription = widget.core.coreLogs.listen((entry) {
      if (!mounted) return;
      setState(() {
        _entries.add(entry);
        if (_entries.length > 512) _entries.removeAt(0);
      });
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (_scrollController.hasClients) {
          _scrollController.jumpTo(_scrollController.position.maxScrollExtent);
        }
      });
    });
  }

  Future<void> _retry() async {
    setState(() => _retrying = true);
    try {
      await widget.core.connect();
    } catch (_) {
      // The terminal already contains the actionable connection error.
    } finally {
      if (mounted) setState(() => _retrying = false);
    }
  }

  void _clear() {
    widget.core.clearCoreLogs();
    setState(_entries.clear);
  }

  @override
  void dispose() {
    _subscription?.cancel();
    _scrollController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: widget.core,
    builder: (context, _) {
      final status = widget.core.status;
      final statusStyle = switch (status) {
        CoreConnectionStatus.connected => RetconStatus.success,
        CoreConnectionStatus.connecting ||
        CoreConnectionStatus.reconnecting => RetconStatus.warning,
        CoreConnectionStatus.disconnected => RetconStatus.error,
      };
      return RetconPanel(
        label: 'Core log terminal',
        padding: const EdgeInsets.all(RetconSpacing.sm),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Row(
              children: [
                RetconBadge(label: 'Core ${status.name}', status: statusStyle),
                const SizedBox(width: RetconSpacing.sm),
                Expanded(
                  child: Text(
                    'Live output from Core launches and connection attempts.',
                    style: Theme.of(context).textTheme.bodySmall,
                  ),
                ),
                TextButton.icon(
                  onPressed: _retrying ? null : _retry,
                  icon: _retrying
                      ? const SizedBox(
                          width: 16,
                          height: 16,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        )
                      : const Icon(Icons.refresh),
                  label: const Text('Retry Core'),
                ),
                TextButton.icon(
                  onPressed: _entries.isEmpty ? null : _clear,
                  icon: const Icon(Icons.clear_all),
                  label: const Text('Clear'),
                ),
              ],
            ),
            if (widget.core.lastConnectionError case final error?) ...[
              const SizedBox(height: RetconSpacing.xs),
              Text(
                'Last connection error: $error',
                key: const Key('core-log-last-error'),
                style: Theme.of(
                  context,
                ).textTheme.bodySmall?.copyWith(color: Colors.amber.shade200),
              ),
            ],
            const SizedBox(height: RetconSpacing.sm),
            Expanded(
              child: Container(
                key: const Key('core-log-output'),
                color: Colors.black,
                padding: const EdgeInsets.all(RetconSpacing.sm),
                child: _entries.isEmpty
                    ? const Center(child: Text('Waiting for Core output…'))
                    : SelectionArea(
                        child: ListView.builder(
                          controller: _scrollController,
                          itemCount: _entries.length,
                          itemBuilder: (context, index) {
                            final entry = _entries[index];
                            return Text(
                              '[${_formatTime(entry.timestamp)}] '
                              '[${entry.source}] ${entry.message}',
                              style: Theme.of(context).textTheme.bodySmall
                                  ?.copyWith(
                                    color: Colors.greenAccent.shade100,
                                    fontFamily: 'monospace',
                                  ),
                            );
                          },
                        ),
                      ),
              ),
            ),
          ],
        ),
      );
    },
  );

  String _formatTime(DateTime time) =>
      '${time.hour.toString().padLeft(2, '0')}:'
      '${time.minute.toString().padLeft(2, '0')}:'
      '${time.second.toString().padLeft(2, '0')}.'
      '${time.millisecond.toString().padLeft(3, '0')}';
}
