import 'package:flutter/material.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

import 'browser_controller.dart';
import 'browser_models.dart';
import 'browser_repository.dart';

class BrowserPanel extends StatefulWidget {
  const BrowserPanel({required this.repository, super.key});

  final BrowserRepository repository;

  @override
  State<BrowserPanel> createState() => _BrowserPanelState();
}

class _BrowserPanelState extends State<BrowserPanel> {
  late final BrowserController _controller;
  late final TextEditingController _address;
  late final TextEditingController _width;
  late final TextEditingController _height;
  late final TextEditingController _selector;
  late final TextEditingController _value;
  late final TextEditingController _takeoverReason;
  BrowserActionKind _action = BrowserActionKind.click;
  BrowserDevicePreset _device = BrowserDevicePreset.responsive;
  bool _fullPage = false;

  @override
  void initState() {
    super.initState();
    _controller = BrowserController(repository: widget.repository);
    _address = TextEditingController(text: 'https://example.com');
    _width = TextEditingController(text: '1280');
    _height = TextEditingController(text: '720');
    _selector = TextEditingController(text: 'main');
    _value = TextEditingController();
    _takeoverReason = TextEditingController(text: 'Manual inspection');
    _controller.addListener(_syncSessionFields);
    _controller.load();
  }

  void _syncSessionFields() {
    final session = _controller.session;
    if (session == null) return;
    final url = session.activeTab?.url;
    if (url != null && url != 'about:blank' && _address.text != url) {
      _address.text = url;
    }
    final viewport = session.viewport;
    if (_width.text != viewport.width.toString()) {
      _width.text = viewport.width.toString();
    }
    if (_height.text != viewport.height.toString()) {
      _height.text = viewport.height.toString();
    }
    _device = viewport.device;
  }

  @override
  void dispose() {
    _controller.removeListener(_syncSessionFields);
    _controller.dispose();
    _address.dispose();
    _width.dispose();
    _height.dispose();
    _selector.dispose();
    _value.dispose();
    _takeoverReason.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: _controller,
    builder: (context, _) {
      if (_controller.loading && !_controller.loaded) {
        return const Center(child: CircularProgressIndicator());
      }
      return RetconPanel(
        label: 'Managed browser',
        padding: EdgeInsets.zero,
        child: Column(
          children: [
            _SessionBar(controller: _controller),
            if (_controller.automationPaused)
              _TakeoverBanner(controller: _controller),
            if (_controller.crashed) _CrashBanner(controller: _controller),
            if (_controller.error case final error?)
              MaterialBanner(
                key: const Key('browser-error-banner'),
                content: Text(error),
                actions: [
                  TextButton(
                    onPressed: _controller.clearError,
                    child: const Text('Dismiss'),
                  ),
                ],
              ),
            if (_controller.session == null)
              Expanded(child: _EmptyBrowser(controller: _controller))
            else ...[
              _TabStrip(controller: _controller),
              _NavigationBar(controller: _controller, address: _address),
              _ViewportBar(
                controller: _controller,
                width: _width,
                height: _height,
                device: _device,
                onDeviceChanged: (value) => setState(() => _device = value),
              ),
              Expanded(
                child: DefaultTabController(
                  length: 3,
                  child: Column(
                    children: [
                      const TabBar(
                        tabs: [
                          Tab(icon: Icon(Icons.web), text: 'Page'),
                          Tab(
                            icon: Icon(Icons.monitor_heart),
                            text: 'Evidence',
                          ),
                          Tab(icon: Icon(Icons.smart_toy), text: 'Automation'),
                        ],
                      ),
                      Expanded(
                        child: TabBarView(
                          children: [
                            _PageView(
                              controller: _controller,
                              fullPage: _fullPage,
                              onFullPageChanged: (value) =>
                                  setState(() => _fullPage = value),
                            ),
                            _EvidenceView(controller: _controller),
                            _AutomationView(
                              controller: _controller,
                              selector: _selector,
                              value: _value,
                              takeoverReason: _takeoverReason,
                              action: _action,
                              onActionChanged: (value) =>
                                  setState(() => _action = value),
                            ),
                          ],
                        ),
                      ),
                    ],
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

class _SessionBar extends StatelessWidget {
  const _SessionBar({required this.controller});
  final BrowserController controller;

  @override
  Widget build(BuildContext context) {
    final session = controller.session;
    return Padding(
      padding: const EdgeInsets.symmetric(
        horizontal: RetconSpacing.sm,
        vertical: RetconSpacing.xs,
      ),
      child: LayoutBuilder(
        builder: (context, constraints) => Row(
          children: [
            RetconBadge(
              key: const Key('browser-runtime-status'),
              label: _runtimeLabel(controller.status),
              status: _runtimeRetconStatus(controller.status),
            ),
            if (session != null) ...[
              const SizedBox(width: RetconSpacing.xs),
              RetconBadge(
                label: 'Profile: ${_compactIdentifier(session.profileId)}',
              ),
              if (constraints.maxWidth >= 920) ...[
                const SizedBox(width: RetconSpacing.xs),
                RetconBadge(
                  label: session.headless ? 'Headless' : 'Headed takeover',
                  status: session.headless
                      ? RetconStatus.neutral
                      : RetconStatus.warning,
                ),
              ],
            ],
            const Spacer(),
            if (session == null)
              FilledButton.icon(
                key: const Key('browser-launch'),
                onPressed: controller.busy ? null : controller.launch,
                icon: const Icon(Icons.rocket_launch),
                label: const Text('Launch isolated session'),
              )
            else ...[
              if (controller.crashed)
                FilledButton.icon(
                  key: const Key('browser-recover'),
                  onPressed: controller.busy ? null : controller.recover,
                  icon: const Icon(Icons.healing),
                  label: const Text('Recover'),
                ),
              TextButton.icon(
                key: const Key('browser-close-session'),
                onPressed: controller.busy ? null : controller.close,
                icon: const Icon(Icons.close),
                label: const Text('Close session'),
              ),
            ],
          ],
        ),
      ),
    );
  }
}

String _compactIdentifier(String value) {
  if (value.length <= 24) return value;
  return '${value.substring(0, 12)}\u2026';
}

class _EmptyBrowser extends StatelessWidget {
  const _EmptyBrowser({required this.controller});
  final BrowserController controller;

  @override
  Widget build(BuildContext context) => Center(
    child: ConstrainedBox(
      constraints: const BoxConstraints(maxWidth: 520),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          const Icon(Icons.language, size: 56),
          const SizedBox(height: RetconSpacing.sm),
          Text(
            'Managed browser session',
            style: Theme.of(context).textTheme.titleLarge,
          ),
          const SizedBox(height: RetconSpacing.xs),
          const Text(
            'Each launch uses an isolated profile. Browser presence and screenshots are inspection context, not verification evidence.',
            textAlign: TextAlign.center,
            key: Key('browser-not-verification-evidence'),
          ),
          const SizedBox(height: RetconSpacing.md),
          FilledButton.icon(
            onPressed: controller.busy ? null : controller.launch,
            icon: const Icon(Icons.rocket_launch),
            label: const Text('Launch browser'),
          ),
        ],
      ),
    ),
  );
}

class _TabStrip extends StatelessWidget {
  const _TabStrip({required this.controller});
  final BrowserController controller;

  @override
  Widget build(BuildContext context) {
    final session = controller.session!;
    return Container(
      height: 42,
      color: RetconColors.titleBarInactive,
      child: Row(
        children: [
          Expanded(
            child: ListView(
              scrollDirection: Axis.horizontal,
              children: [
                for (final tab in session.tabs)
                  Padding(
                    padding: const EdgeInsets.only(left: RetconSpacing.xxs),
                    child: InputChip(
                      key: Key('browser-tab-${tab.id}'),
                      selected: tab.id == session.activeTabId,
                      avatar: tab.status == BrowserTabStatus.loading
                          ? const SizedBox.square(
                              dimension: 14,
                              child: CircularProgressIndicator(strokeWidth: 2),
                            )
                          : const Icon(Icons.public, size: 16),
                      label: ConstrainedBox(
                        constraints: const BoxConstraints(maxWidth: 180),
                        child: Text(tab.title, overflow: TextOverflow.ellipsis),
                      ),
                      onPressed: () => controller.selectTab(tab.id),
                      onDeleted:
                          controller.capabilities.multipleTabs &&
                              session.tabs.length > 1
                          ? () => controller.closeTab(tab.id)
                          : null,
                    ),
                  ),
              ],
            ),
          ),
          IconButton(
            key: const Key('browser-new-tab'),
            tooltip: controller.capabilities.multipleTabs
                ? 'New tab'
                : 'Multiple tabs require the Phase 24 Core adapter',
            onPressed: controller.capabilities.multipleTabs && !controller.busy
                ? controller.newTab
                : null,
            icon: const Icon(Icons.add),
          ),
        ],
      ),
    );
  }
}

class _NavigationBar extends StatelessWidget {
  const _NavigationBar({required this.controller, required this.address});
  final BrowserController controller;
  final TextEditingController address;

  @override
  Widget build(BuildContext context) {
    final tab = controller.activeTab!;
    final controlsEnabled = !controller.busy && !controller.automationPaused;
    return Padding(
      padding: const EdgeInsets.all(RetconSpacing.xs),
      child: Row(
        children: [
          IconButton(
            key: const Key('browser-back'),
            tooltip: 'Back',
            onPressed:
                controlsEnabled &&
                    controller.capabilities.historyNavigation &&
                    tab.canGoBack
                ? controller.back
                : null,
            icon: const Icon(Icons.arrow_back),
          ),
          IconButton(
            key: const Key('browser-forward'),
            tooltip: 'Forward',
            onPressed:
                controlsEnabled &&
                    controller.capabilities.historyNavigation &&
                    tab.canGoForward
                ? controller.forward
                : null,
            icon: const Icon(Icons.arrow_forward),
          ),
          IconButton(
            key: const Key('browser-reload'),
            tooltip: 'Reload',
            onPressed: controlsEnabled && controller.capabilities.reload
                ? controller.reload
                : null,
            icon: const Icon(Icons.refresh),
          ),
          IconButton(
            key: const Key('browser-stop-loading'),
            tooltip: 'Stop loading',
            onPressed:
                controlsEnabled &&
                    controller.capabilities.stopLoading &&
                    tab.status == BrowserTabStatus.loading
                ? controller.stopLoading
                : null,
            icon: const Icon(Icons.close),
          ),
          Expanded(
            child: RetconTextField(
              key: const Key('browser-address'),
              label: 'Address',
              hint: 'https://localhost:3000',
              controller: address,
              enabled: controlsEnabled,
            ),
          ),
          const SizedBox(width: RetconSpacing.xs),
          FilledButton(
            key: const Key('browser-go'),
            onPressed: controlsEnabled
                ? () => controller.navigate(address.text)
                : null,
            child: const Text('Go'),
          ),
        ],
      ),
    );
  }
}

class _ViewportBar extends StatelessWidget {
  const _ViewportBar({
    required this.controller,
    required this.width,
    required this.height,
    required this.device,
    required this.onDeviceChanged,
  });
  final BrowserController controller;
  final TextEditingController width;
  final TextEditingController height;
  final BrowserDevicePreset device;
  final ValueChanged<BrowserDevicePreset> onDeviceChanged;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.symmetric(horizontal: RetconSpacing.sm),
    child: Row(
      children: [
        const Icon(Icons.devices, size: 18),
        const SizedBox(width: RetconSpacing.xs),
        SizedBox(
          width: 150,
          child: DropdownButtonFormField<BrowserDevicePreset>(
            key: const Key('browser-device'),
            initialValue: device,
            isExpanded: true,
            decoration: const InputDecoration(labelText: 'Device'),
            items: [
              for (final preset in BrowserDevicePreset.values)
                DropdownMenuItem(value: preset, child: Text(preset.name)),
            ],
            onChanged:
                controller.capabilities.viewportAndDevice && !controller.busy
                ? (value) {
                    if (value != null) onDeviceChanged(value);
                  }
                : null,
          ),
        ),
        const SizedBox(width: RetconSpacing.xs),
        SizedBox(
          width: 90,
          child: RetconTextField(label: 'Width', controller: width),
        ),
        const Padding(
          padding: EdgeInsets.symmetric(horizontal: RetconSpacing.xs),
          child: Text('×'),
        ),
        SizedBox(
          width: 90,
          child: RetconTextField(label: 'Height', controller: height),
        ),
        const SizedBox(width: RetconSpacing.xs),
        OutlinedButton(
          key: const Key('browser-apply-viewport'),
          onPressed:
              controller.capabilities.viewportAndDevice && !controller.busy
              ? () {
                  final parsedWidth = int.tryParse(width.text);
                  final parsedHeight = int.tryParse(height.text);
                  if (parsedWidth != null && parsedHeight != null) {
                    controller.setViewport(
                      BrowserViewport(
                        width: parsedWidth,
                        height: parsedHeight,
                        device: device,
                        deviceScaleFactor: device == BrowserDevicePreset.mobile
                            ? 2
                            : 1,
                      ),
                    );
                  }
                }
              : null,
          child: const Text('Apply viewport'),
        ),
        const Spacer(),
        Text(
          '${controller.session!.viewport.width} × ${controller.session!.viewport.height}',
        ),
      ],
    ),
  );
}

class _PageView extends StatelessWidget {
  const _PageView({
    required this.controller,
    required this.fullPage,
    required this.onFullPageChanged,
  });
  final BrowserController controller;
  final bool fullPage;
  final ValueChanged<bool> onFullPageChanged;

  @override
  Widget build(BuildContext context) {
    final session = controller.session!;
    final tab = session.activeTab!;
    return ListView(
      key: const Key('browser-page-view'),
      padding: const EdgeInsets.all(RetconSpacing.md),
      children: [
        RetconPanel(
          label: 'Managed page',
          recessed: true,
          child: SizedBox(
            height: 170,
            child: Center(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  const Icon(Icons.web_asset, size: 44),
                  const SizedBox(height: RetconSpacing.xs),
                  Text(
                    tab.title,
                    style: Theme.of(context).textTheme.titleLarge,
                  ),
                  Text(tab.url, key: const Key('browser-active-url')),
                  const SizedBox(height: RetconSpacing.xs),
                  const Text(
                    'Browser context is available for inspection only. Verification gates remain separate.',
                    key: Key('browser-inspection-only'),
                  ),
                ],
              ),
            ),
          ),
        ),
        const SizedBox(height: RetconSpacing.sm),
        if (session.previewMetadata.isNotEmpty)
          RetconPanel(
            key: const Key('browser-preview-metadata'),
            label: 'Dev server preview metadata',
            recessed: true,
            child: Text(session.previewMetadata.toString()),
          ),
        if (session.history.isNotEmpty) ...[
          const SizedBox(height: RetconSpacing.sm),
          RetconPanel(
            key: const Key('browser-session-history'),
            label: 'Durable session history',
            recessed: true,
            child: Column(
              children: [
                for (final event in session.history.reversed.take(5))
                  ListTile(
                    dense: true,
                    leading: const Icon(Icons.history),
                    title: Text(event.kind),
                    subtitle: Text(
                      '${event.actor} · ${event.createdAt.toLocal()} · ${event.details}',
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
              ],
            ),
          ),
        ],
        const SizedBox(height: RetconSpacing.sm),
        Row(
          children: [
            Switch.adaptive(value: fullPage, onChanged: onFullPageChanged),
            const Text('Full-page screenshot'),
            const SizedBox(width: RetconSpacing.sm),
            FilledButton.icon(
              key: const Key('browser-screenshot'),
              onPressed: controller.capabilities.screenshots && !controller.busy
                  ? () => controller.captureScreenshot(fullPage: fullPage)
                  : null,
              icon: const Icon(Icons.screenshot),
              label: const Text('Capture screenshot'),
            ),
          ],
        ),
        if (controller.snapshot.screenshotPath case final path?) ...[
          const SizedBox(height: RetconSpacing.sm),
          ListTile(
            leading: const Icon(Icons.image),
            title: const Text('Latest screenshot artifact'),
            subtitle: Text(path),
          ),
        ],
      ],
    );
  }
}

class _EvidenceView extends StatelessWidget {
  const _EvidenceView({required this.controller});
  final BrowserController controller;

  @override
  Widget build(BuildContext context) {
    final entries = controller.visibleEvidence;
    return Column(
      children: [
        Padding(
          padding: const EdgeInsets.all(RetconSpacing.xs),
          child: Row(
            children: [
              Expanded(
                child: SingleChildScrollView(
                  scrollDirection: Axis.horizontal,
                  child: Row(
                    children: [
                      for (final kind in BrowserEvidenceKind.values)
                        Padding(
                          padding: const EdgeInsets.only(
                            right: RetconSpacing.xs,
                          ),
                          child: ChoiceChip(
                            key: Key('browser-evidence-${kind.name}'),
                            label: Text(_evidenceLabel(kind)),
                            selected: controller.evidenceKind == kind,
                            onSelected: (_) => controller.showEvidence(kind),
                          ),
                        ),
                    ],
                  ),
                ),
              ),
              IconButton(
                key: const Key('browser-refresh-evidence'),
                tooltip: 'Refresh evidence',
                onPressed: controller.busy ? null : controller.refreshEvidence,
                icon: const Icon(Icons.refresh),
              ),
            ],
          ),
        ),
        Expanded(
          child: entries.isEmpty
              ? Center(
                  child: Text(
                    'No ${_evidenceLabel(controller.evidenceKind).toLowerCase()} captured.',
                  ),
                )
              : ListView.separated(
                  key: const Key('browser-evidence-list'),
                  padding: const EdgeInsets.all(RetconSpacing.sm),
                  itemCount: entries.length,
                  separatorBuilder: (_, _) => const Divider(),
                  itemBuilder: (context, index) {
                    final entry = entries[index];
                    return ListTile(
                      dense: true,
                      leading: Icon(_evidenceIcon(entry.kind)),
                      title: Text(entry.summary),
                      subtitle: Text(
                        '${entry.createdAt.toLocal()}\n${entry.details}',
                        maxLines: 3,
                        overflow: TextOverflow.ellipsis,
                      ),
                      trailing: RetconBadge(label: entry.level),
                    );
                  },
                ),
        ),
      ],
    );
  }
}

class _AutomationView extends StatelessWidget {
  const _AutomationView({
    required this.controller,
    required this.selector,
    required this.value,
    required this.takeoverReason,
    required this.action,
    required this.onActionChanged,
  });
  final BrowserController controller;
  final TextEditingController selector;
  final TextEditingController value;
  final TextEditingController takeoverReason;
  final BrowserActionKind action;
  final ValueChanged<BrowserActionKind> onActionChanged;

  @override
  Widget build(BuildContext context) {
    final enabled =
        controller.capabilities.automation &&
        !controller.busy &&
        !controller.automationPaused;
    final history = controller.session!.takeoverHistory.reversed.toList();
    return ListView(
      key: const Key('browser-automation-view'),
      padding: const EdgeInsets.all(RetconSpacing.md),
      children: [
        RetconPanel(
          label: 'Safe page action',
          recessed: true,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              const Text(
                'Actions are limited to click, fill, key press, and text read. Fill values are masked from history and evidence.',
                key: Key('browser-safe-action-notice'),
              ),
              const SizedBox(height: RetconSpacing.sm),
              Row(
                children: [
                  SizedBox(
                    width: 160,
                    child: DropdownButtonFormField<BrowserActionKind>(
                      key: const Key('browser-action-kind'),
                      initialValue: action,
                      isExpanded: true,
                      decoration: const InputDecoration(labelText: 'Action'),
                      items: [
                        for (final kind in BrowserActionKind.values)
                          DropdownMenuItem(value: kind, child: Text(kind.name)),
                      ],
                      onChanged: enabled
                          ? (value) {
                              if (value != null) onActionChanged(value);
                            }
                          : null,
                    ),
                  ),
                  const SizedBox(width: RetconSpacing.sm),
                  Expanded(
                    child: RetconTextField(
                      key: const Key('browser-action-selector'),
                      label: 'Selector',
                      hint: '[data-testid="submit"]',
                      controller: selector,
                      enabled: enabled,
                    ),
                  ),
                  if (action == BrowserActionKind.fill ||
                      action == BrowserActionKind.press ||
                      action == BrowserActionKind.select) ...[
                    const SizedBox(width: RetconSpacing.sm),
                    Expanded(
                      child: RetconTextField(
                        key: const Key('browser-action-value'),
                        label: action == BrowserActionKind.fill
                            ? 'Value (masked)'
                            : action == BrowserActionKind.select
                            ? 'Option value'
                            : 'Key',
                        controller: value,
                        obscureText: action == BrowserActionKind.fill,
                        enabled: enabled,
                      ),
                    ),
                  ],
                  const SizedBox(width: RetconSpacing.sm),
                  FilledButton.icon(
                    key: const Key('browser-run-action'),
                    onPressed: enabled
                        ? () => controller.performAction(
                            BrowserAutomationAction(
                              kind: action,
                              selector: selector.text,
                              value: value.text,
                            ),
                          )
                        : null,
                    icon: const Icon(Icons.play_arrow),
                    label: const Text('Run action'),
                  ),
                ],
              ),
              if (controller.lastActionResult case final result?) ...[
                const SizedBox(height: RetconSpacing.xs),
                Text(result),
              ],
            ],
          ),
        ),
        const SizedBox(height: RetconSpacing.sm),
        RetconPanel(
          label: 'Manual takeover',
          recessed: true,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              RetconTextField(
                key: const Key('browser-takeover-reason'),
                label: 'Reason',
                controller: takeoverReason,
              ),
              const SizedBox(height: RetconSpacing.xs),
              Wrap(
                spacing: RetconSpacing.xs,
                children: [
                  OutlinedButton.icon(
                    key: const Key('browser-pause-automation'),
                    onPressed: !controller.automationPaused && !controller.busy
                        ? () => controller.pauseAutomation(
                            reason: takeoverReason.text,
                          )
                        : null,
                    icon: const Icon(Icons.pause),
                    label: const Text('Pause automation'),
                  ),
                  OutlinedButton.icon(
                    key: const Key('browser-open-headed'),
                    onPressed:
                        controller.capabilities.headedTakeover &&
                            !controller.busy
                        ? controller.openHeadedTakeover
                        : null,
                    icon: const Icon(Icons.open_in_new),
                    label: const Text('Open headed'),
                  ),
                  FilledButton.icon(
                    key: const Key('browser-resume-automation'),
                    onPressed: controller.automationPaused && !controller.busy
                        ? controller.resumeAutomation
                        : null,
                    icon: const Icon(Icons.play_arrow),
                    label: const Text('Resume automation'),
                  ),
                ],
              ),
              const SizedBox(height: RetconSpacing.sm),
              Text(
                'Takeover history',
                style: Theme.of(context).textTheme.titleSmall,
              ),
              if (history.isEmpty)
                const Text('No manual takeover intervals.')
              else
                for (final interval in history.take(10))
                  ListTile(
                    dense: true,
                    leading: Icon(
                      interval.endedAt == null ? Icons.pause : Icons.history,
                    ),
                    title: Text(interval.reason),
                    subtitle: Text(
                      '${interval.startedAt.toLocal()} → ${interval.endedAt?.toLocal() ?? 'active'}',
                    ),
                    trailing: interval.openedHeaded
                        ? const RetconBadge(label: 'headed')
                        : null,
                  ),
            ],
          ),
        ),
      ],
    );
  }
}

class _TakeoverBanner extends StatelessWidget {
  const _TakeoverBanner({required this.controller});
  final BrowserController controller;

  @override
  Widget build(BuildContext context) => MaterialBanner(
    key: const Key('browser-takeover-banner'),
    leading: const Icon(Icons.pan_tool),
    content: const Text(
      'Automation is paused for manual takeover. Navigation and page actions are locked until resume.',
    ),
    actions: [
      TextButton(
        onPressed: controller.busy ? null : controller.resumeAutomation,
        child: const Text('Resume'),
      ),
    ],
  );
}

class _CrashBanner extends StatelessWidget {
  const _CrashBanner({required this.controller});
  final BrowserController controller;

  @override
  Widget build(BuildContext context) => MaterialBanner(
    key: const Key('browser-crash-banner'),
    leading: const Icon(Icons.error_outline),
    content: Text(
      '${controller.session?.crashMessage ?? 'The browser crashed.'} Recovery keeps browser evidence and starts a fresh process for the isolated profile.',
    ),
    actions: [
      TextButton(
        onPressed: controller.busy ? null : controller.recover,
        child: const Text('Recover browser'),
      ),
    ],
  );
}

String _runtimeLabel(BrowserRuntimeStatus status) => switch (status) {
  BrowserRuntimeStatus.stopped => 'Stopped',
  BrowserRuntimeStatus.launching => 'Launching',
  BrowserRuntimeStatus.running => 'Running',
  BrowserRuntimeStatus.paused => 'Automation paused',
  BrowserRuntimeStatus.crashed => 'Crashed',
  BrowserRuntimeStatus.recovering => 'Recovering',
};

RetconStatus _runtimeRetconStatus(BrowserRuntimeStatus status) =>
    switch (status) {
      BrowserRuntimeStatus.running => RetconStatus.success,
      BrowserRuntimeStatus.paused ||
      BrowserRuntimeStatus.recovering => RetconStatus.warning,
      BrowserRuntimeStatus.crashed => RetconStatus.error,
      _ => RetconStatus.neutral,
    };

String _evidenceLabel(BrowserEvidenceKind kind) => switch (kind) {
  BrowserEvidenceKind.screenshots => 'Screenshots',
  BrowserEvidenceKind.console => 'Console',
  BrowserEvidenceKind.network => 'Network',
  BrowserEvidenceKind.errors => 'Errors',
  BrowserEvidenceKind.accessibility => 'Accessibility',
  BrowserEvidenceKind.performance => 'Performance',
  BrowserEvidenceKind.artifacts => 'Artifacts',
};

IconData _evidenceIcon(BrowserEvidenceKind kind) => switch (kind) {
  BrowserEvidenceKind.screenshots => Icons.screenshot,
  BrowserEvidenceKind.console => Icons.terminal,
  BrowserEvidenceKind.network => Icons.wifi,
  BrowserEvidenceKind.errors => Icons.error_outline,
  BrowserEvidenceKind.accessibility => Icons.accessibility_new,
  BrowserEvidenceKind.performance => Icons.speed,
  BrowserEvidenceKind.artifacts => Icons.inventory_2,
};
