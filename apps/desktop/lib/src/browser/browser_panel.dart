import 'package:flutter/material.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

import '../core_client.dart';
import 'browser_controller.dart';

/// Minimal browser workspace panel over Core `browser.*` spike RPCs.
class BrowserPanel extends StatefulWidget {
  const BrowserPanel({required this.core, super.key});

  final CoreClient core;

  @override
  State<BrowserPanel> createState() => _BrowserPanelState();
}

class _BrowserPanelState extends State<BrowserPanel> {
  late final BrowserController _controller;
  late final TextEditingController _dir;
  late final TextEditingController _url;

  @override
  void initState() {
    super.initState();
    _controller = BrowserController(widget.core);
    _dir = TextEditingController(text: _controller.serviceDir);
    _url = TextEditingController(text: 'https://example.com');
  }

  @override
  void dispose() {
    _dir.dispose();
    _url.dispose();
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
          label: 'Browser',
          padding: const EdgeInsets.all(RetconSpacing.md),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Row(
                children: [
                  RetconBadge(
                    label: _controller.running ? 'Running' : 'Stopped',
                    status: _controller.running
                        ? RetconStatus.success
                        : RetconStatus.neutral,
                  ),
                  const Spacer(),
                  TextButton.icon(
                    onPressed: connected &&
                            !_controller.busy &&
                            !_controller.running
                        ? _controller.start
                        : null,
                    icon: const Icon(Icons.play_arrow),
                    label: const Text('Start'),
                  ),
                  TextButton.icon(
                    onPressed: connected &&
                            !_controller.busy &&
                            _controller.running
                        ? _controller.stop
                        : null,
                    icon: const Icon(Icons.stop),
                    label: const Text('Stop'),
                  ),
                  TextButton.icon(
                    onPressed: connected &&
                            !_controller.busy &&
                            _controller.running
                        ? _controller.refreshStatus
                        : null,
                    icon: const Icon(Icons.health_and_safety),
                    label: const Text('Status'),
                  ),
                ],
              ),
              const SizedBox(height: RetconSpacing.sm),
              if (!connected)
                Text(
                  'Connect to Retcon Core to control the browser service.',
                  style: theme.textTheme.bodyMedium,
                )
              else ...[
                RetconTextField(
                  label: 'Browser service directory',
                  hint: r'...\Retcon\apps\browser-service',
                  controller: _dir,
                  onChanged: _controller.setServiceDir,
                ),
                const SizedBox(height: RetconSpacing.sm),
                Row(
                  children: [
                    Expanded(
                      child: RetconTextField(
                        label: 'Navigate',
                        hint: 'https://example.com',
                        controller: _url,
                      ),
                    ),
                    const SizedBox(width: RetconSpacing.sm),
                    FilledButton(
                      onPressed: _controller.busy
                          ? null
                          : () => _controller.navigate(_url.text),
                      child: const Text('Go'),
                    ),
                  ],
                ),
                if (_controller.error != null) ...[
                  const SizedBox(height: RetconSpacing.sm),
                  Text(
                    _controller.error!,
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: theme.colorScheme.error,
                    ),
                  ),
                ],
                if (_controller.lastResult != null) ...[
                  const SizedBox(height: RetconSpacing.sm),
                  Text(
                    _controller.lastResult!,
                    style: theme.textTheme.bodySmall,
                    maxLines: 4,
                    overflow: TextOverflow.ellipsis,
                  ),
                ],
                const SizedBox(height: RetconSpacing.sm),
                Text('Events', style: theme.textTheme.titleSmall),
                const SizedBox(height: RetconSpacing.xs),
                Expanded(
                  child: _controller.eventLog.isEmpty
                      ? Text(
                          'Browser events appear here after the service starts.',
                          style: theme.textTheme.bodySmall,
                        )
                      : ListView.separated(
                          itemCount: _controller.eventLog.length,
                          separatorBuilder: (_, _) =>
                              const SizedBox(height: RetconSpacing.xxs),
                          itemBuilder: (context, index) => Text(
                            _controller.eventLog[index],
                            style: theme.textTheme.bodySmall,
                            maxLines: 3,
                            overflow: TextOverflow.ellipsis,
                          ),
                        ),
                ),
              ],
            ],
          ),
        );
      },
    );
  }
}
