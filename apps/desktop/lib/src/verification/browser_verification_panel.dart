import 'dart:io';

import 'package:flutter/material.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

import 'browser_verification_controller.dart';
import 'browser_verification_models.dart';

class BrowserVerificationPanel extends StatelessWidget {
  const BrowserVerificationPanel({required this.controller, super.key});

  final BrowserVerificationController controller;

  @override
  Widget build(BuildContext context) => DefaultTabController(
    length: 5,
    child: AnimatedBuilder(
      animation: controller,
      builder: (context, _) => Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          _BrowserRunToolbar(controller: controller),
          const TabBar(
            isScrollable: true,
            tabs: [
              Tab(icon: Icon(Icons.tune), text: 'Definition'),
              Tab(icon: Icon(Icons.timeline), text: 'Timeline'),
              Tab(icon: Icon(Icons.compare), text: 'Visual'),
              Tab(icon: Icon(Icons.accessibility_new), text: 'Accessibility'),
              Tab(icon: Icon(Icons.fact_check), text: 'Evidence'),
            ],
          ),
          if (controller.error != null)
            MaterialBanner(
              content: Text(
                controller.error!,
                key: const Key('browser-verification-error'),
              ),
              actions: [
                TextButton(
                  onPressed: controller.load,
                  child: const Text('Retry'),
                ),
              ],
            ),
          Expanded(
            child: controller.loading && !controller.loaded
                ? const Center(child: CircularProgressIndicator())
                : TabBarView(
                    children: [
                      _DefinitionEditor(controller: controller),
                      _TimelineView(run: controller.latestRun),
                      _VisualReview(controller: controller),
                      _AccessibilityReview(run: controller.latestRun),
                      _BrowserEvidence(controller: controller),
                    ],
                  ),
          ),
        ],
      ),
    ),
  );
}

class _BrowserRunToolbar extends StatelessWidget {
  const _BrowserRunToolbar({required this.controller});
  final BrowserVerificationController controller;

  @override
  Widget build(BuildContext context) {
    final run = controller.latestRun;
    return Semantics(
      container: true,
      label: 'Browser verification controls',
      child: Padding(
        padding: const EdgeInsets.all(RetconSpacing.sm),
        child: Wrap(
          spacing: RetconSpacing.sm,
          runSpacing: RetconSpacing.xs,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: [
            RetconBadge(
              key: const Key('browser-verification-status'),
              label: controller.loading
                  ? 'Loading'
                  : run == null
                  ? controller.configured
                        ? 'Not run'
                        : 'Not configured'
                  : _runLabel(run.status),
              status: _runRetconStatus(run?.status),
            ),
            FilledButton.icon(
              key: const Key('run-browser-verification'),
              onPressed: controller.canRun ? controller.run : null,
              icon: const Icon(Icons.play_arrow),
              label: const Text('Run browser verification'),
            ),
            if (controller.running)
              OutlinedButton.icon(
                key: const Key('cancel-browser-verification'),
                onPressed: controller.cancel,
                icon: const Icon(Icons.stop),
                label: const Text('Cancel'),
              ),
            if (run != null)
              Text(
                'Attempt ${run.attempt} · ${run.timeline.length} events',
                key: const Key('browser-verification-run-summary'),
              ),
          ],
        ),
      ),
    );
  }
}

class _DefinitionEditor extends StatefulWidget {
  const _DefinitionEditor({required this.controller});
  final BrowserVerificationController controller;

  @override
  State<_DefinitionEditor> createState() => _DefinitionEditorState();
}

class _DefinitionEditorState extends State<_DefinitionEditor> {
  BrowserVerificationDefinition? _draft;

  BrowserVerificationDefinition get _value => _draft!;

  void _change(BrowserVerificationDefinition value) =>
      setState(() => _draft = value);

  @override
  Widget build(BuildContext context) {
    _draft ??= widget.controller.definition;
    if (_draft == null) {
      return Center(
        child: FilledButton.icon(
          key: const Key('configure-browser-verification'),
          onPressed: () => setState(
            () => _draft = BrowserVerificationDefinition(
              id: 'browser-${widget.controller.taskId}',
              taskId: widget.controller.taskId,
              name: 'Browser verification',
              targetUrl: 'http://127.0.0.1:3000',
            ),
          ),
          icon: const Icon(Icons.add),
          label: const Text('Configure browser verification'),
        ),
      );
    }
    final value = _value;
    return ListView(
      key: const Key('browser-verification-definition'),
      padding: const EdgeInsets.all(RetconSpacing.md),
      children: [
        Text(
          'Target and execution',
          style: Theme.of(context).textTheme.titleMedium,
        ),
        const SizedBox(height: RetconSpacing.sm),
        LayoutBuilder(
          builder: (context, constraints) {
            final width = constraints.maxWidth >= 760
                ? (constraints.maxWidth - RetconSpacing.sm) / 2
                : constraints.maxWidth;
            return Wrap(
              spacing: RetconSpacing.sm,
              runSpacing: RetconSpacing.sm,
              children: [
                SizedBox(
                  width: width,
                  child: TextFormField(
                    key: const Key('browser-target-url'),
                    initialValue: value.targetUrl,
                    keyboardType: TextInputType.url,
                    decoration: const InputDecoration(labelText: 'Target URL'),
                    onChanged: (text) =>
                        _draft = _value.copyWith(targetUrl: text),
                  ),
                ),
                SizedBox(
                  width: width,
                  child: TextFormField(
                    key: const Key('browser-required-server'),
                    initialValue: value.requiredServerId,
                    decoration: const InputDecoration(
                      labelText: 'Required server configuration',
                      helperText: 'Verification uses its live server instance.',
                    ),
                    onChanged: (text) =>
                        _draft = _value.copyWith(requiredServerId: text),
                  ),
                ),
                SizedBox(
                  width: width,
                  child: TextFormField(
                    key: const Key('browser-timeout'),
                    initialValue: value.timeout.inSeconds.toString(),
                    keyboardType: TextInputType.number,
                    decoration: const InputDecoration(
                      labelText: 'Timeout (seconds)',
                    ),
                    onChanged: (text) {
                      final seconds = int.tryParse(text);
                      if (seconds != null && seconds > 0) {
                        _draft = _value.copyWith(
                          timeout: Duration(seconds: seconds),
                        );
                      }
                    },
                  ),
                ),
                SizedBox(
                  width: width,
                  child: TextFormField(
                    key: const Key('browser-retries'),
                    initialValue: value.retryCount.toString(),
                    keyboardType: TextInputType.number,
                    decoration: const InputDecoration(labelText: 'Retries'),
                    onChanged: (text) {
                      final retries = int.tryParse(text);
                      if (retries != null && retries >= 0) {
                        _draft = _value.copyWith(retryCount: retries);
                      }
                    },
                  ),
                ),
              ],
            );
          },
        ),
        SwitchListTile(
          key: const Key('browser-required-gate'),
          contentPadding: EdgeInsets.zero,
          title: const Text('Required for task completion'),
          subtitle: const Text(
            'Failures, console errors, critical accessibility issues, and unreviewed visual changes block completion.',
          ),
          value: value.required,
          onChanged: (required) => _change(value.copyWith(required: required)),
        ),
        SwitchListTile(
          key: const Key('browser-fail-on-accessibility'),
          contentPadding: EdgeInsets.zero,
          title: const Text('Block on critical accessibility issues'),
          subtitle: const Text(
            'Warnings remain reviewable evidence without blocking completion.',
          ),
          value: value.failOnAccessibility,
          onChanged: (enabled) =>
              _change(value.copyWith(failOnAccessibility: enabled)),
        ),
        const Divider(height: RetconSpacing.lg),
        _ViewportEditor(value: value, onChanged: _change),
        const Divider(height: RetconSpacing.lg),
        _VisualPolicyEditor(value: value, onChanged: _change),
        const Divider(height: RetconSpacing.lg),
        Row(
          children: [
            Expanded(
              child: Text(
                'Ordered steps and assertions',
                style: Theme.of(context).textTheme.titleMedium,
              ),
            ),
            IconButton(
              key: const Key('add-browser-step'),
              tooltip: 'Add browser step',
              onPressed: () {
                final index = value.steps.length + 1;
                _change(
                  value.copyWith(
                    steps: [
                      ...value.steps,
                      BrowserVerificationStep(
                        id: 'step-$index',
                        label: 'Step $index',
                        kind: BrowserStepKind.click,
                      ),
                    ],
                  ),
                );
              },
              icon: const Icon(Icons.add),
            ),
          ],
        ),
        if (value.steps.isEmpty)
          const Text('Add the navigation and interaction steps to verify.'),
        for (var index = 0; index < value.steps.length; index++)
          _StepEditor(
            key: ValueKey(value.steps[index].id),
            step: value.steps[index],
            index: index,
            count: value.steps.length,
            onChanged: (step) {
              final steps = [...value.steps]..[index] = step;
              _change(value.copyWith(steps: steps));
            },
            onMove: (offset) {
              final target = index + offset;
              if (target < 0 || target >= value.steps.length) return;
              final steps = [...value.steps];
              final step = steps.removeAt(index);
              steps.insert(target, step);
              _change(value.copyWith(steps: steps));
            },
            onRemove: () {
              final steps = [...value.steps]..removeAt(index);
              _change(value.copyWith(steps: steps));
            },
          ),
        const SizedBox(height: RetconSpacing.md),
        Align(
          alignment: Alignment.centerLeft,
          child: FilledButton.icon(
            key: const Key('save-browser-definition'),
            onPressed: widget.controller.saving
                ? null
                : () => widget.controller.saveDefinition(_value),
            icon: const Icon(Icons.save),
            label: Text(
              widget.controller.saving ? 'Saving…' : 'Save definition',
            ),
          ),
        ),
      ],
    );
  }
}

class _ViewportEditor extends StatelessWidget {
  const _ViewportEditor({required this.value, required this.onChanged});
  final BrowserVerificationDefinition value;
  final ValueChanged<BrowserVerificationDefinition> onChanged;

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Text(
        'Responsive viewports',
        style: Theme.of(context).textTheme.titleMedium,
      ),
      const SizedBox(height: RetconSpacing.xs),
      Wrap(
        spacing: RetconSpacing.xs,
        runSpacing: RetconSpacing.xs,
        children: [
          for (final viewport in value.viewports)
            InputChip(
              label: Text('${viewport.label} · ${viewport.dimensions}'),
              onDeleted: value.viewports.length == 1
                  ? null
                  : () => onChanged(
                      value.copyWith(
                        viewports: value.viewports
                            .where((item) => item.id != viewport.id)
                            .toList(),
                      ),
                    ),
            ),
          ActionChip(
            key: const Key('add-mobile-viewport'),
            avatar: const Icon(Icons.add, size: 18),
            label: const Text('Add mobile'),
            onPressed: value.viewports.any((item) => item.id == 'mobile')
                ? null
                : () => onChanged(
                    value.copyWith(
                      viewports: [
                        ...value.viewports,
                        const BrowserViewport(
                          id: 'mobile',
                          label: 'Mobile',
                          width: 390,
                          height: 844,
                          deviceScaleFactor: 3,
                        ),
                      ],
                    ),
                  ),
          ),
        ],
      ),
    ],
  );
}

class _VisualPolicyEditor extends StatelessWidget {
  const _VisualPolicyEditor({required this.value, required this.onChanged});
  final BrowserVerificationDefinition value;
  final ValueChanged<BrowserVerificationDefinition> onChanged;

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Text(
        'Visual comparison policy',
        style: Theme.of(context).textTheme.titleMedium,
      ),
      const SizedBox(height: RetconSpacing.xs),
      SizedBox(
        width: 260,
        child: TextFormField(
          key: const Key('visual-difference-threshold'),
          initialValue: (value.visualThreshold * 100).toStringAsFixed(2),
          keyboardType: const TextInputType.numberWithOptions(decimal: true),
          decoration: const InputDecoration(
            labelText: 'Allowed difference (%)',
          ),
          onChanged: (text) {
            final percentage = double.tryParse(text);
            if (percentage != null && percentage >= 0) {
              onChanged(value.copyWith(visualThreshold: percentage / 100));
            }
          },
        ),
      ),
      const SizedBox(height: RetconSpacing.xs),
      LayoutBuilder(
        builder: (context, constraints) {
          final width = constraints.maxWidth >= 620
              ? (constraints.maxWidth - RetconSpacing.sm) / 2
              : constraints.maxWidth;
          return Wrap(
            spacing: RetconSpacing.sm,
            runSpacing: RetconSpacing.xs,
            children: [
              SizedBox(
                width: width,
                child: TextFormField(
                  key: const Key('visual-mask-selectors'),
                  initialValue: value.maskSelectors.join(', '),
                  decoration: const InputDecoration(
                    labelText: 'Mask selectors',
                    hintText: '.clock, [data-live]',
                  ),
                  onChanged: (text) => onChanged(
                    value.copyWith(maskSelectors: _selectors(text)),
                  ),
                ),
              ),
              SizedBox(
                width: width,
                child: TextFormField(
                  key: const Key('visual-ignore-selectors'),
                  initialValue: value.ignoreSelectors.join(', '),
                  decoration: const InputDecoration(
                    labelText: 'Ignore selectors',
                    hintText: '.animation, video',
                  ),
                  onChanged: (text) => onChanged(
                    value.copyWith(ignoreSelectors: _selectors(text)),
                  ),
                ),
              ),
            ],
          );
        },
      ),
      const SizedBox(height: RetconSpacing.xs),
      const Text(
        'Selectors configure dynamic content before capture. Coordinate masks '
        'reported by the runner remain visible in comparison evidence.',
      ),
    ],
  );
}

class _StepEditor extends StatelessWidget {
  const _StepEditor({
    required this.step,
    required this.index,
    required this.count,
    required this.onChanged,
    required this.onMove,
    required this.onRemove,
    super.key,
  });

  final BrowserVerificationStep step;
  final int index;
  final int count;
  final ValueChanged<BrowserVerificationStep> onChanged;
  final ValueChanged<int> onMove;
  final VoidCallback onRemove;

  @override
  Widget build(BuildContext context) => Card(
    child: ExpansionTile(
      key: Key('browser-step-${step.id}'),
      initiallyExpanded: true,
      leading: CircleAvatar(radius: 14, child: Text('${index + 1}')),
      title: Text(step.label),
      subtitle: Text(step.kind.name),
      trailing: PopupMenuButton<String>(
        tooltip: 'Reorder or remove step',
        onSelected: (action) {
          if (action == 'up') onMove(-1);
          if (action == 'down') onMove(1);
          if (action == 'remove') onRemove();
        },
        itemBuilder: (context) => [
          PopupMenuItem(
            value: 'up',
            enabled: index > 0,
            child: const ListTile(
              leading: Icon(Icons.arrow_upward),
              title: Text('Move up'),
            ),
          ),
          PopupMenuItem(
            value: 'down',
            enabled: index < count - 1,
            child: const ListTile(
              leading: Icon(Icons.arrow_downward),
              title: Text('Move down'),
            ),
          ),
          const PopupMenuItem(
            value: 'remove',
            child: ListTile(
              leading: Icon(Icons.delete_outline),
              title: Text('Remove'),
            ),
          ),
        ],
      ),
      children: [
        Padding(
          padding: const EdgeInsets.all(RetconSpacing.sm),
          child: Column(
            children: [
              LayoutBuilder(
                builder: (context, constraints) => Wrap(
                  spacing: RetconSpacing.sm,
                  runSpacing: RetconSpacing.sm,
                  children: [
                    SizedBox(
                      width: constraints.maxWidth > 680
                          ? 220
                          : constraints.maxWidth,
                      child: DropdownButtonFormField<BrowserStepKind>(
                        isExpanded: true,
                        initialValue: step.kind,
                        decoration: const InputDecoration(labelText: 'Action'),
                        items: [
                          for (final kind in BrowserStepKind.values)
                            DropdownMenuItem(
                              value: kind,
                              child: Text(kind.name),
                            ),
                        ],
                        onChanged: (kind) {
                          if (kind != null) {
                            onChanged(step.copyWith(kind: kind));
                          }
                        },
                      ),
                    ),
                    SizedBox(
                      width: constraints.maxWidth > 680
                          ? 260
                          : constraints.maxWidth,
                      child: TextFormField(
                        initialValue: step.label,
                        decoration: const InputDecoration(labelText: 'Label'),
                        onChanged: (text) =>
                            onChanged(step.copyWith(label: text)),
                      ),
                    ),
                    SizedBox(
                      width: constraints.maxWidth > 680
                          ? 260
                          : constraints.maxWidth,
                      child: TextFormField(
                        initialValue: step.target,
                        decoration: const InputDecoration(
                          labelText: 'Selector / URL / key',
                        ),
                        onChanged: (text) =>
                            onChanged(step.copyWith(target: text)),
                      ),
                    ),
                    SizedBox(
                      width: constraints.maxWidth > 680
                          ? 220
                          : constraints.maxWidth,
                      child: TextFormField(
                        initialValue: step.value,
                        decoration: const InputDecoration(labelText: 'Value'),
                        onChanged: (text) =>
                            onChanged(step.copyWith(value: text)),
                      ),
                    ),
                  ],
                ),
              ),
              const SizedBox(height: RetconSpacing.sm),
              Align(
                alignment: Alignment.centerLeft,
                child: Text(
                  'Assertions',
                  style: Theme.of(context).textTheme.titleSmall,
                ),
              ),
              for (
                var assertionIndex = 0;
                assertionIndex < step.assertions.length;
                assertionIndex++
              )
                _AssertionEditor(
                  assertion: step.assertions[assertionIndex],
                  onChanged: (assertion) {
                    final assertions = [...step.assertions]
                      ..[assertionIndex] = assertion;
                    onChanged(step.copyWith(assertions: assertions));
                  },
                  onRemove: () {
                    final assertions = [...step.assertions]
                      ..removeAt(assertionIndex);
                    onChanged(step.copyWith(assertions: assertions));
                  },
                ),
              Align(
                alignment: Alignment.centerLeft,
                child: TextButton.icon(
                  key: Key('add-assertion-${step.id}'),
                  onPressed: () {
                    final next = step.assertions.length + 1;
                    onChanged(
                      step.copyWith(
                        assertions: [
                          ...step.assertions,
                          BrowserAssertion(
                            id: '${step.id}-assertion-$next',
                            kind: BrowserAssertionKind.element,
                            expected: 'visible',
                          ),
                        ],
                      ),
                    );
                  },
                  icon: const Icon(Icons.add),
                  label: const Text('Add assertion'),
                ),
              ),
            ],
          ),
        ),
      ],
    ),
  );
}

class _AssertionEditor extends StatelessWidget {
  const _AssertionEditor({
    required this.assertion,
    required this.onChanged,
    required this.onRemove,
  });
  final BrowserAssertion assertion;
  final ValueChanged<BrowserAssertion> onChanged;
  final VoidCallback onRemove;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(top: RetconSpacing.xs),
    child: Wrap(
      spacing: RetconSpacing.xs,
      runSpacing: RetconSpacing.xs,
      crossAxisAlignment: WrapCrossAlignment.center,
      children: [
        SizedBox(
          width: 180,
          child: DropdownButtonFormField<BrowserAssertionKind>(
            isExpanded: true,
            initialValue: assertion.kind,
            decoration: const InputDecoration(labelText: 'Assertion'),
            items: [
              for (final kind in BrowserAssertionKind.values)
                DropdownMenuItem(value: kind, child: Text(kind.name)),
            ],
            onChanged: (kind) {
              if (kind != null) onChanged(assertion.copyWith(kind: kind));
            },
          ),
        ),
        SizedBox(
          width: 220,
          child: TextFormField(
            initialValue: assertion.target,
            decoration: const InputDecoration(labelText: 'Target'),
            onChanged: (text) => onChanged(assertion.copyWith(target: text)),
          ),
        ),
        SizedBox(
          width: 220,
          child: TextFormField(
            initialValue: assertion.expected,
            decoration: const InputDecoration(labelText: 'Expected'),
            onChanged: (text) => onChanged(assertion.copyWith(expected: text)),
          ),
        ),
        FilterChip(
          label: const Text('Required'),
          selected: assertion.required,
          onSelected: (required) =>
              onChanged(assertion.copyWith(required: required)),
        ),
        IconButton(
          tooltip: 'Remove assertion',
          onPressed: onRemove,
          icon: const Icon(Icons.remove_circle_outline),
        ),
      ],
    ),
  );
}

class _TimelineView extends StatelessWidget {
  const _TimelineView({required this.run});
  final BrowserVerificationRun? run;

  @override
  Widget build(BuildContext context) {
    if (run == null) {
      return const Center(child: Text('Run verification to build a timeline.'));
    }
    final events = [...run!.timeline]
      ..sort((a, b) => a.createdAt.compareTo(b.createdAt));
    return ListView.separated(
      key: const Key('browser-verification-timeline'),
      padding: const EdgeInsets.all(RetconSpacing.md),
      itemCount: events.length,
      separatorBuilder: (_, _) => const SizedBox(height: RetconSpacing.xs),
      itemBuilder: (context, index) {
        final event = events[index];
        return Semantics(
          label: '${index + 1}. ${event.kind.name}: ${event.label}',
          child: Card(
            child: ListTile(
              leading: CircleAvatar(
                child: Icon(_timelineIcon(event.kind), size: 18),
              ),
              title: Text(event.label),
              subtitle: Text(
                [
                  event.kind.name,
                  _time(event.createdAt),
                  if (event.duration != null)
                    '${event.duration!.inMilliseconds} ms',
                  if (event.details.isNotEmpty) _details(event.details),
                ].join(' · '),
              ),
              trailing: event.passed == null
                  ? null
                  : Icon(
                      event.passed! ? Icons.check_circle : Icons.error,
                      color: event.passed!
                          ? Theme.of(context).colorScheme.primary
                          : Theme.of(context).colorScheme.error,
                    ),
            ),
          ),
        );
      },
    );
  }
}

class _VisualReview extends StatelessWidget {
  const _VisualReview({required this.controller});
  final BrowserVerificationController controller;

  @override
  Widget build(BuildContext context) {
    final comparisons = [
      for (final run in controller.history)
        for (final comparison in run.visualComparisons) (run, comparison),
    ];
    if (comparisons.isEmpty) {
      return const Center(
        child: Text(
          'No visual captures yet. Add screenshot assertions and run verification.',
        ),
      );
    }
    return ListView.builder(
      key: const Key('browser-visual-review'),
      padding: const EdgeInsets.all(RetconSpacing.md),
      itemCount: comparisons.length,
      itemBuilder: (context, index) {
        final (run, comparison) = comparisons[index];
        return Card(
          margin: const EdgeInsets.only(bottom: RetconSpacing.sm),
          child: Padding(
            padding: const EdgeInsets.all(RetconSpacing.sm),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Wrap(
                  spacing: RetconSpacing.xs,
                  runSpacing: RetconSpacing.xs,
                  crossAxisAlignment: WrapCrossAlignment.center,
                  children: [
                    Text(
                      comparison.viewport.label,
                      style: Theme.of(context).textTheme.titleMedium,
                    ),
                    RetconBadge(label: comparison.viewport.dimensions),
                    RetconBadge(
                      label: _visualStatusLabel(comparison.status),
                      status: comparison.changed
                          ? RetconStatus.error
                          : RetconStatus.success,
                    ),
                    Text(
                      '${(comparison.difference * 100).toStringAsFixed(2)}% difference '
                      '· ${(comparison.threshold * 100).toStringAsFixed(2)}% threshold '
                      '· ${comparison.method.name}',
                    ),
                    if (comparison.changed)
                      FilledButton.icon(
                        key: Key('approve-baseline-${comparison.id}'),
                        onPressed: () => controller.approveBaseline(comparison),
                        icon: const Icon(Icons.approval),
                        label: const Text('Approve current as baseline'),
                      ),
                  ],
                ),
                const SizedBox(height: RetconSpacing.sm),
                LayoutBuilder(
                  builder: (context, constraints) {
                    final width = constraints.maxWidth >= 760
                        ? (constraints.maxWidth - RetconSpacing.sm * 2) / 3
                        : constraints.maxWidth;
                    return Wrap(
                      spacing: RetconSpacing.sm,
                      runSpacing: RetconSpacing.sm,
                      children: [
                        SizedBox(
                          width: width,
                          child: _ArtifactPreview(
                            label: 'Baseline',
                            artifact: comparison.baseline,
                          ),
                        ),
                        SizedBox(
                          width: width,
                          child: _ArtifactPreview(
                            label: 'Current',
                            artifact: comparison.current,
                          ),
                        ),
                        SizedBox(
                          width: width,
                          child: _ArtifactPreview(
                            label: 'Difference',
                            artifact: comparison.diff,
                          ),
                        ),
                      ],
                    );
                  },
                ),
                if (comparison.masks.isNotEmpty) ...[
                  const SizedBox(height: RetconSpacing.xs),
                  Text(
                    'Ignored dynamic regions: ${comparison.masks.map((mask) => mask.label).join(', ')}',
                  ),
                ],
                Text(
                  'Run ${run.attempt} · ${_time(comparison.createdAt)}',
                  style: Theme.of(context).textTheme.bodySmall,
                ),
              ],
            ),
          ),
        );
      },
    );
  }
}

class _ArtifactPreview extends StatelessWidget {
  const _ArtifactPreview({required this.label, required this.artifact});
  final String label;
  final BrowserVisualArtifact? artifact;

  @override
  Widget build(BuildContext context) {
    final path = artifact?.localPath;
    return SizedBox(
      height: 220,
      child: Semantics(
        label: '$label visual artifact',
        image: artifact != null,
        child: Container(
          decoration: BoxDecoration(
            border: Border.all(color: Theme.of(context).dividerColor),
            borderRadius: BorderRadius.circular(4),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Padding(
                padding: const EdgeInsets.all(RetconSpacing.xs),
                child: Text(
                  label,
                  style: Theme.of(context).textTheme.titleSmall,
                ),
              ),
              Expanded(
                child: path != null
                    ? Image.file(
                        File(path),
                        fit: BoxFit.contain,
                        errorBuilder: (_, _, _) =>
                            _ArtifactFallback(artifact: artifact),
                      )
                    : _ArtifactFallback(artifact: artifact),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _ArtifactFallback extends StatelessWidget {
  const _ArtifactFallback({required this.artifact});
  final BrowserVisualArtifact? artifact;

  @override
  Widget build(BuildContext context) => Center(
    child: Padding(
      padding: const EdgeInsets.all(RetconSpacing.sm),
      child: Text(
        artifact == null
            ? 'Not captured'
            : 'Artifact ${artifact!.hash.substring(0, artifact!.hash.length.clamp(0, 12))}…',
        textAlign: TextAlign.center,
      ),
    ),
  );
}

class _AccessibilityReview extends StatelessWidget {
  const _AccessibilityReview({required this.run});
  final BrowserVerificationRun? run;

  @override
  Widget build(BuildContext context) {
    final issues =
        run?.accessibilityIssues ?? const <BrowserAccessibilityIssue>[];
    if (run == null) {
      return const Center(
        child: Text('Run verification to inspect accessibility.'),
      );
    }
    if (issues.isEmpty) {
      return const Center(
        child: Text('No accessibility issues were detected.'),
      );
    }
    return ListView(
      key: const Key('browser-accessibility-review'),
      padding: const EdgeInsets.all(RetconSpacing.md),
      children: [
        Wrap(
          spacing: RetconSpacing.xs,
          runSpacing: RetconSpacing.xs,
          children: [
            for (final severity in AccessibilitySeverity.values)
              RetconBadge(
                label:
                    '${_severityLabel(severity)}: ${issues.where((issue) => issue.severity == severity).length}',
                status: _severityStatus(severity),
              ),
          ],
        ),
        const SizedBox(height: RetconSpacing.sm),
        for (final category in AccessibilityCategory.values)
          if (issues.any((issue) => issue.category == category))
            _AccessibilityGroup(
              category: category,
              issues: issues
                  .where((issue) => issue.category == category)
                  .toList(),
            ),
      ],
    );
  }
}

class _AccessibilityGroup extends StatelessWidget {
  const _AccessibilityGroup({required this.category, required this.issues});
  final AccessibilityCategory category;
  final List<BrowserAccessibilityIssue> issues;

  @override
  Widget build(BuildContext context) => Card(
    child: ExpansionTile(
      initiallyExpanded: issues.any(
        (issue) => issue.severity == AccessibilitySeverity.critical,
      ),
      leading: Icon(_accessibilityIcon(category)),
      title: Text('${_categoryLabel(category)} (${issues.length})'),
      children: [
        for (final issue in issues)
          Semantics(
            label: '${_severityLabel(issue.severity)}: ${issue.message}',
            child: ListTile(
              leading: Icon(
                issue.severity == AccessibilitySeverity.critical
                    ? Icons.error
                    : issue.severity == AccessibilitySeverity.warning
                    ? Icons.warning
                    : Icons.info,
                color: _severityColor(context, issue.severity),
              ),
              title: Text(issue.message),
              subtitle: Text(
                [
                  if (issue.selector != null) issue.selector!,
                  if (issue.help != null) issue.help!,
                ].join('\n'),
              ),
            ),
          ),
      ],
    ),
  );
}

class _BrowserEvidence extends StatelessWidget {
  const _BrowserEvidence({required this.controller});
  final BrowserVerificationController controller;

  @override
  Widget build(BuildContext context) {
    final run = controller.latestRun;
    return ListView(
      key: const Key('browser-verification-evidence'),
      padding: const EdgeInsets.all(RetconSpacing.md),
      children: [
        Text(
          'Task completion evidence',
          style: Theme.of(context).textTheme.titleMedium,
        ),
        const SizedBox(height: RetconSpacing.xs),
        if (!controller.configured)
          const Text('Browser verification is not configured for this task.')
        else if (controller.completionBlocker case final blocker?)
          _EvidenceNotice(
            key: const Key('browser-completion-blocker'),
            icon: Icons.block,
            title: 'Blocking',
            message: blocker,
            color: Theme.of(context).colorScheme.error,
          )
        else
          _EvidenceNotice(
            icon: Icons.verified,
            title: 'Passed',
            message: 'Browser evidence satisfies the task completion gate.',
            color: Theme.of(context).colorScheme.primary,
          ),
        if (run?.status == BrowserRunStatus.needsReview) ...[
          const SizedBox(height: RetconSpacing.sm),
          Wrap(
            spacing: RetconSpacing.xs,
            runSpacing: RetconSpacing.xs,
            children: [
              FilledButton.icon(
                key: const Key('approve-browser-review'),
                onPressed:
                    run!.hasCriticalAccessibility ||
                        run.consoleErrors.isNotEmpty ||
                        run.hasUnapprovedVisualChanges
                    ? null
                    : () => controller.review(approve: true),
                icon: const Icon(Icons.verified),
                label: const Text('Approve evidence'),
              ),
              OutlinedButton.icon(
                key: const Key('reject-browser-review'),
                onPressed: () => controller.review(
                  approve: false,
                  reason: 'Evidence rejected during desktop review.',
                ),
                icon: const Icon(Icons.block),
                label: const Text('Reject evidence'),
              ),
            ],
          ),
        ],
        if (controller.warningCount > 0) ...[
          const SizedBox(height: RetconSpacing.sm),
          _EvidenceNotice(
            key: const Key('browser-completion-warning'),
            icon: Icons.warning,
            title: '${controller.warningCount} warnings',
            message: 'Warnings do not block completion but should be reviewed.',
            color: Colors.amber,
          ),
        ],
        const Divider(height: RetconSpacing.lg),
        Text('Console errors', style: Theme.of(context).textTheme.titleMedium),
        const SizedBox(height: RetconSpacing.xs),
        if (run == null || run.consoleErrors.isEmpty)
          const Text('No console errors recorded.')
        else
          for (final error in run.consoleErrors)
            ListTile(
              key: const Key('browser-console-error'),
              leading: Icon(
                Icons.error_outline,
                color: Theme.of(context).colorScheme.error,
              ),
              title: SelectableText(error),
            ),
        const Divider(height: RetconSpacing.lg),
        Text(
          'Evidence history',
          style: Theme.of(context).textTheme.titleMedium,
        ),
        const SizedBox(height: RetconSpacing.xs),
        if (controller.history.isEmpty)
          const Text('No browser verification runs yet.'),
        for (final historical in controller.history)
          ListTile(
            leading: Icon(
              historical.status == BrowserRunStatus.passed
                  ? Icons.check_circle
                  : Icons.cancel,
            ),
            title: Text(
              '${_runLabel(historical.status)} · attempt ${historical.attempt}',
            ),
            subtitle: Text(
              '${historical.timeline.length} events · '
              '${historical.visualComparisons.length} visual comparisons · '
              '${historical.accessibilityIssues.length} accessibility issues',
            ),
          ),
      ],
    );
  }
}

class _EvidenceNotice extends StatelessWidget {
  const _EvidenceNotice({
    required this.icon,
    required this.title,
    required this.message,
    required this.color,
    super.key,
  });
  final IconData icon;
  final String title;
  final String message;
  final Color color;

  @override
  Widget build(BuildContext context) => Semantics(
    liveRegion: true,
    child: Container(
      padding: const EdgeInsets.all(RetconSpacing.sm),
      decoration: BoxDecoration(
        border: Border.all(color: color),
        borderRadius: BorderRadius.circular(4),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(icon, color: color),
          const SizedBox(width: RetconSpacing.sm),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(title, style: Theme.of(context).textTheme.titleSmall),
                Text(message),
              ],
            ),
          ),
        ],
      ),
    ),
  );
}

String _runLabel(BrowserRunStatus status) => switch (status) {
  BrowserRunStatus.queued => 'Queued',
  BrowserRunStatus.running => 'Running',
  BrowserRunStatus.needsReview => 'Needs review',
  BrowserRunStatus.passed => 'Passed',
  BrowserRunStatus.failed => 'Failed',
  BrowserRunStatus.cancelled => 'Cancelled',
};

RetconStatus _runRetconStatus(BrowserRunStatus? status) => switch (status) {
  BrowserRunStatus.passed => RetconStatus.success,
  BrowserRunStatus.failed || BrowserRunStatus.cancelled => RetconStatus.error,
  BrowserRunStatus.running || BrowserRunStatus.queued => RetconStatus.warning,
  BrowserRunStatus.needsReview => RetconStatus.warning,
  null => RetconStatus.neutral,
};

String _visualStatusLabel(VisualComparisonStatus status) => switch (status) {
  VisualComparisonStatus.passed => 'Within threshold',
  VisualComparisonStatus.changed => 'Review required',
  VisualComparisonStatus.missingBaseline => 'Baseline missing',
  VisualComparisonStatus.approved => 'Baseline approved',
};

IconData _timelineIcon(BrowserTimelineKind kind) => switch (kind) {
  BrowserTimelineKind.navigation => Icons.navigation,
  BrowserTimelineKind.interaction => Icons.touch_app,
  BrowserTimelineKind.network => Icons.lan,
  BrowserTimelineKind.console => Icons.terminal,
  BrowserTimelineKind.screenshot => Icons.screenshot,
  BrowserTimelineKind.assertion => Icons.fact_check,
  BrowserTimelineKind.accessibility => Icons.accessibility_new,
  BrowserTimelineKind.error => Icons.error,
  BrowserTimelineKind.takeover => Icons.pan_tool,
  BrowserTimelineKind.completion => Icons.flag,
};

String _severityLabel(AccessibilitySeverity severity) => switch (severity) {
  AccessibilitySeverity.info => 'Info',
  AccessibilitySeverity.warning => 'Warnings',
  AccessibilitySeverity.critical => 'Critical',
};

RetconStatus _severityStatus(AccessibilitySeverity severity) =>
    switch (severity) {
      AccessibilitySeverity.info => RetconStatus.neutral,
      AccessibilitySeverity.warning => RetconStatus.warning,
      AccessibilitySeverity.critical => RetconStatus.error,
    };

Color _severityColor(BuildContext context, AccessibilitySeverity severity) =>
    switch (severity) {
      AccessibilitySeverity.info => Theme.of(context).colorScheme.primary,
      AccessibilitySeverity.warning => Colors.amber,
      AccessibilitySeverity.critical => Theme.of(context).colorScheme.error,
    };

String _categoryLabel(AccessibilityCategory category) => switch (category) {
  AccessibilityCategory.labels => 'Missing labels',
  AccessibilityCategory.contrast => 'Contrast',
  AccessibilityCategory.keyboard => 'Keyboard',
  AccessibilityCategory.headings => 'Heading structure',
  AccessibilityCategory.landmarks => 'Landmarks',
  AccessibilityCategory.forms => 'Form labeling',
  AccessibilityCategory.focus => 'Focus visibility',
};

IconData _accessibilityIcon(AccessibilityCategory category) =>
    switch (category) {
      AccessibilityCategory.labels => Icons.label_off,
      AccessibilityCategory.contrast => Icons.contrast,
      AccessibilityCategory.keyboard => Icons.keyboard,
      AccessibilityCategory.headings => Icons.title,
      AccessibilityCategory.landmarks => Icons.account_tree,
      AccessibilityCategory.forms => Icons.dynamic_form,
      AccessibilityCategory.focus => Icons.center_focus_strong,
    };

String _time(DateTime value) =>
    '${value.hour.toString().padLeft(2, '0')}:'
    '${value.minute.toString().padLeft(2, '0')}:'
    '${value.second.toString().padLeft(2, '0')}';

String _details(Map<String, Object?> details) => details.entries
    .take(3)
    .map((entry) => '${entry.key}: ${entry.value}')
    .join(', ');

List<String> _selectors(String value) => value
    .split(',')
    .map((selector) => selector.trim())
    .where((selector) => selector.isNotEmpty)
    .toList();
