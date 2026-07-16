import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

/// Persistent, versioned workspace layout.  This stays deliberately independent
/// of widgets so a project layout can be restored before the desktop is drawn.
class WorkspaceLayout {
  const WorkspaceLayout({
    required this.root,
    this.floatingPanels = const [],
    this.closedPanels = const [],
    this.version = currentVersion,
  });

  static const currentVersion = 1;
  final WorkspaceNode root;
  final List<FloatingPanel> floatingPanels;
  final List<PanelDefinition> closedPanels;
  final int version;

  factory WorkspaceLayout.initial() => WorkspaceLayout(
    root: SplitGroup.horizontal(
      first: TabGroup(panels: const [PanelDefinition.explorer]),
      second: TabGroup(panels: const [PanelDefinition.workspace]),
      fraction: .24,
    ),
  );

  Map<String, Object?> toJson() => {
    'version': currentVersion,
    'root': root.toJson(),
    'floatingPanels': floatingPanels.map((panel) => panel.toJson()).toList(),
    'closedPanels': closedPanels.map((panel) => panel.toJson()).toList(),
  };

  factory WorkspaceLayout.fromJson(Map<String, dynamic> json) {
    // Version 0 stored only the root; accepting it makes old prototypes safe.
    final root = WorkspaceNode.fromJson(json['root'] as Map<String, dynamic>);
    return WorkspaceLayout(
      root: root,
      floatingPanels: (json['floatingPanels'] as List<dynamic>? ?? const [])
          .map((item) => FloatingPanel.fromJson(item as Map<String, dynamic>))
          .toList(),
      closedPanels: (json['closedPanels'] as List<dynamic>? ?? const [])
          .map((item) => PanelDefinition.fromJson(item as Map<String, dynamic>))
          .toList(),
    );
  }
}

sealed class WorkspaceNode {
  const WorkspaceNode();
  Map<String, Object?> toJson();

  factory WorkspaceNode.fromJson(Map<String, dynamic> json) =>
      switch (json['type']) {
        'tabs' => TabGroup.fromJson(json),
        'split' => SplitGroup.fromJson(json),
        _ => throw FormatException('Unknown workspace node: ${json['type']}'),
      };
}

class TabGroup extends WorkspaceNode {
  const TabGroup({required this.panels, this.activeIndex = 0});
  final List<PanelDefinition> panels;
  final int activeIndex;
  PanelDefinition get active => panels[activeIndex.clamp(0, panels.length - 1)];

  @override
  Map<String, Object?> toJson() => {
    'type': 'tabs',
    'activeIndex': activeIndex,
    'panels': panels.map((panel) => panel.toJson()).toList(),
  };
  factory TabGroup.fromJson(Map<String, dynamic> json) => TabGroup(
    activeIndex: (json['activeIndex'] as num?)?.toInt() ?? 0,
    panels: (json['panels'] as List<dynamic>)
        .map((item) => PanelDefinition.fromJson(item as Map<String, dynamic>))
        .toList(),
  );
}

enum SplitAxis { horizontal, vertical }

class SplitGroup extends WorkspaceNode {
  const SplitGroup({
    required this.axis,
    required this.first,
    required this.second,
    required this.fraction,
  });
  const SplitGroup.horizontal({
    required WorkspaceNode first,
    required WorkspaceNode second,
    double fraction = .5,
  }) : this(
         axis: SplitAxis.horizontal,
         first: first,
         second: second,
         fraction: fraction,
       );
  const SplitGroup.vertical({
    required WorkspaceNode first,
    required WorkspaceNode second,
    double fraction = .5,
  }) : this(
         axis: SplitAxis.vertical,
         first: first,
         second: second,
         fraction: fraction,
       );
  final SplitAxis axis;
  final WorkspaceNode first;
  final WorkspaceNode second;
  final double fraction;
  @override
  Map<String, Object?> toJson() => {
    'type': 'split',
    'axis': axis.name,
    'fraction': fraction,
    'first': first.toJson(),
    'second': second.toJson(),
  };
  factory SplitGroup.fromJson(Map<String, dynamic> json) => SplitGroup(
    axis: json['axis'] == 'vertical'
        ? SplitAxis.vertical
        : SplitAxis.horizontal,
    fraction: ((json['fraction'] as num?)?.toDouble() ?? .5).clamp(.15, .85),
    first: WorkspaceNode.fromJson(json['first'] as Map<String, dynamic>),
    second: WorkspaceNode.fromJson(json['second'] as Map<String, dynamic>),
  );
}

class PanelDefinition {
  const PanelDefinition({
    required this.id,
    required this.title,
    required this.icon,
  });
  static const explorer = PanelDefinition(
    id: 'explorer',
    title: 'Project explorer',
    icon: 'folder',
  );
  static const workspace = PanelDefinition(
    id: 'workspace',
    title: 'Workspace',
    icon: 'dashboard',
  );
  static const terminal = PanelDefinition(
    id: 'terminal',
    title: 'Terminal',
    icon: 'terminal',
  );
  static const browser = PanelDefinition(
    id: 'browser',
    title: 'Browser',
    icon: 'language',
  );
  final String id;
  final String title;
  final String icon;
  Map<String, Object?> toJson() => {'id': id, 'title': title, 'icon': icon};
  factory PanelDefinition.fromJson(Map<String, dynamic> json) =>
      PanelDefinition(
        id: json['id'] as String,
        title: json['title'] as String,
        icon: json['icon'] as String? ?? 'dashboard',
      );
}

class FloatingPanel {
  const FloatingPanel({
    required this.panel,
    required this.rect,
    this.detached = false,
  });
  final PanelDefinition panel;
  final Rect rect;
  final bool detached;
  Map<String, Object?> toJson() => {
    'panel': panel.toJson(),
    'x': rect.left,
    'y': rect.top,
    'width': rect.width,
    'height': rect.height,
    'detached': detached,
  };
  factory FloatingPanel.fromJson(Map<String, dynamic> json) => FloatingPanel(
    panel: PanelDefinition.fromJson(json['panel'] as Map<String, dynamic>),
    rect: Rect.fromLTWH(
      (json['x'] as num).toDouble(),
      (json['y'] as num).toDouble(),
      (json['width'] as num).toDouble(),
      (json['height'] as num).toDouble(),
    ),
    detached: json['detached'] as bool? ?? false,
  );
}

abstract interface class WorkspaceStore {
  Future<WorkspaceLayout?> read();
  Future<void> write(WorkspaceLayout layout);
}

class FileWorkspaceStore implements WorkspaceStore {
  FileWorkspaceStore({String? path})
    : _file = File(
        path ??
            '${Directory.current.path}${Platform.pathSeparator}.retcon-workspace.json',
      );
  final File _file;
  @override
  Future<WorkspaceLayout?> read() async {
    if (!await _file.exists()) return null;
    try {
      return WorkspaceLayout.fromJson(
        jsonDecode(await _file.readAsString()) as Map<String, dynamic>,
      );
    } on Object {
      return null;
    } // Corrupt layouts fall back to a known-safe default.
  }

  @override
  Future<void> write(WorkspaceLayout layout) =>
      _file.writeAsString(jsonEncode(layout.toJson()), flush: true);
}

class WorkspaceController extends ChangeNotifier {
  WorkspaceController({WorkspaceStore? store})
    : _store = store ?? FileWorkspaceStore();
  final WorkspaceStore _store;
  WorkspaceLayout _layout = WorkspaceLayout.initial();
  bool _disposed = false;
  int _mutationEpoch = 0;
  static const _maxClosedPanels = 32;

  WorkspaceLayout get layout => _layout;

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }

  Future<void> restore() async {
    final epoch = _mutationEpoch;
    WorkspaceLayout restored;
    try {
      restored = await _store.read() ?? WorkspaceLayout.initial();
    } on Object {
      restored = WorkspaceLayout.initial();
    }
    // Skip applying restore if the user already mutated layout or we were disposed.
    if (_disposed || epoch != _mutationEpoch) return;
    _layout = restored;
    _notify();
  }

  Future<void> reset() async {
    _mutationEpoch += 1;
    _layout = WorkspaceLayout.initial();
    await _save();
  }

  Future<void> reopenLast() async {
    if (_layout.closedPanels.isEmpty) return;
    _mutationEpoch += 1;
    final panel = _layout.closedPanels.last;
    _layout = WorkspaceLayout(
      root: _append(_layout.root, panel),
      floatingPanels: _layout.floatingPanels,
      closedPanels: _layout.closedPanels.sublist(
        0,
        _layout.closedPanels.length - 1,
      ),
    );
    await _save();
  }

  Future<void> close(PanelDefinition panel) async {
    _mutationEpoch += 1;
    final removed = _remove(_layout.root, panel.id);
    final closed = [..._layout.closedPanels, panel];
    _layout = WorkspaceLayout(
      root: removed.node,
      floatingPanels: _layout.floatingPanels
          .where((item) => item.panel.id != panel.id)
          .toList(),
      closedPanels: closed.length > _maxClosedPanels
          ? closed.sublist(closed.length - _maxClosedPanels)
          : closed,
    );
    await _save();
  }

  Future<void> float(
    PanelDefinition panel,
    Size workspace, {
    bool detached = false,
  }) async {
    _mutationEpoch += 1;
    final existing = _layout.floatingPanels
        .where((item) => item.panel.id == panel.id)
        .toList();
    if (existing.isNotEmpty) {
      // Singleton panels: focus/reuse the existing floating instance.
      _layout = WorkspaceLayout(
        root: _remove(_layout.root, panel.id).node,
        closedPanels: _layout.closedPanels,
        floatingPanels: _layout.floatingPanels
            .map(
              (item) => item.panel.id == panel.id
                  ? FloatingPanel(
                      panel: item.panel,
                      rect: item.rect,
                      detached: detached,
                    )
                  : item,
            )
            .toList(),
      );
      await _save();
      return;
    }
    final safe = Rect.fromLTWH(48, 48, 360, 260).shift(
      Offset(workspace.width > 500 ? 80 : 0, workspace.height > 400 ? 40 : 0),
    );
    _layout = WorkspaceLayout(
      root: _remove(_layout.root, panel.id).node,
      closedPanels: _layout.closedPanels,
      floatingPanels: [
        ..._layout.floatingPanels,
        FloatingPanel(panel: panel, rect: safe, detached: detached),
      ],
    );
    await _save();
  }

  Future<void> dock(FloatingPanel floating) async {
    _mutationEpoch += 1;
    _layout = WorkspaceLayout(
      root: _append(_layout.root, floating.panel),
      closedPanels: _layout.closedPanels,
      floatingPanels: _layout.floatingPanels
          .where((item) => item != floating)
          .toList(),
    );
    await _save();
  }

  Future<void> activateTab(TabGroup group, int index) async {
    if (index < 0 || index >= group.panels.length) return;
    if (group.activeIndex == index) return;
    _mutationEpoch += 1;
    _layout = WorkspaceLayout(
      root: _setActiveTab(_layout.root, group, index),
      floatingPanels: _layout.floatingPanels,
      closedPanels: _layout.closedPanels,
    );
    await _save();
  }

  Future<void> _save() async {
    _notify();
    await _store.write(_layout);
  }

  void _notify() {
    if (!_disposed) notifyListeners();
  }

  WorkspaceNode _setActiveTab(
    WorkspaceNode node,
    TabGroup target,
    int index,
  ) => switch (node) {
    TabGroup group => identical(group, target) || _sameTabGroup(group, target)
        ? TabGroup(panels: group.panels, activeIndex: index)
        : group,
    SplitGroup split => SplitGroup(
      axis: split.axis,
      first: _setActiveTab(split.first, target, index),
      second: _setActiveTab(split.second, target, index),
      fraction: split.fraction,
    ),
  };

  bool _sameTabGroup(TabGroup left, TabGroup right) {
    if (left.panels.length != right.panels.length) return false;
    for (var i = 0; i < left.panels.length; i++) {
      if (left.panels[i].id != right.panels[i].id) return false;
    }
    return true;
  }

  WorkspaceNode _append(WorkspaceNode node, PanelDefinition panel) =>
      switch (node) {
        TabGroup group => TabGroup(
          panels: [...group.panels, panel],
          activeIndex: group.panels.length,
        ),
        SplitGroup split => SplitGroup(
          axis: split.axis,
          first: split.first,
          second: _append(split.second, panel),
          fraction: split.fraction,
        ),
      };

  ({WorkspaceNode node, bool removed}) _remove(WorkspaceNode node, String id) {
    switch (node) {
      case TabGroup group:
        final panels = group.panels.where((panel) => panel.id != id).toList();
        if (panels.length == group.panels.length) {
          return (node: group, removed: false);
        }
        if (panels.isEmpty) {
          return (
            node: const TabGroup(panels: [PanelDefinition.workspace]),
            removed: true,
          );
        }
        return (
          node: TabGroup(
            panels: panels,
            activeIndex: group.activeIndex.clamp(0, panels.length - 1),
          ),
          removed: true,
        );
      case SplitGroup split:
        final first = _remove(split.first, id);
        final second = _remove(split.second, id);
        return (
          node: SplitGroup(
            axis: split.axis,
            first: first.node,
            second: second.node,
            fraction: split.fraction,
          ),
          removed: first.removed || second.removed,
        );
    }
  }
}

class DockingWorkspace extends StatefulWidget {
  const DockingWorkspace({super.key, required this.controller});
  final WorkspaceController controller;
  @override
  State<DockingWorkspace> createState() => _DockingWorkspaceState();
}

class _DockingWorkspaceState extends State<DockingWorkspace> {
  @override
  void initState() {
    super.initState();
    unawaited(widget.controller.restore());
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: widget.controller,
    builder: (context, _) => LayoutBuilder(
      builder: (context, bounds) => Stack(
        children: [
          Positioned.fill(
            child: _NodeView(
              node: widget.controller.layout.root,
              controller: widget.controller,
              workspaceSize: bounds.biggest,
            ),
          ),
          for (final floating in widget.controller.layout.floatingPanels)
            _FloatingView(
              key: ValueKey('floating-${floating.panel.id}'),
              floating: floating,
              controller: widget.controller,
            ),
        ],
      ),
    ),
  );
}

class _NodeView extends StatelessWidget {
  const _NodeView({
    required this.node,
    required this.controller,
    required this.workspaceSize,
  });
  final WorkspaceNode node;
  final WorkspaceController controller;
  final Size workspaceSize;
  @override
  Widget build(BuildContext context) => switch (node) {
    TabGroup group => _TabGroupView(
      group: group,
      controller: controller,
      workspaceSize: workspaceSize,
    ),
    SplitGroup split => Flex(
      direction: split.axis == SplitAxis.horizontal
          ? Axis.horizontal
          : Axis.vertical,
      children: [
        Expanded(
          flex: (split.fraction * 100).round(),
          child: _NodeView(
            node: split.first,
            controller: controller,
            workspaceSize: workspaceSize,
          ),
        ),
        const SizedBox(width: RetconSpacing.xs, height: RetconSpacing.xs),
        Expanded(
          flex: ((1 - split.fraction) * 100).round(),
          child: _NodeView(
            node: split.second,
            controller: controller,
            workspaceSize: workspaceSize,
          ),
        ),
      ],
    ),
  };
}

class _TabGroupView extends StatelessWidget {
  const _TabGroupView({
    required this.group,
    required this.controller,
    required this.workspaceSize,
  });
  final TabGroup group;
  final WorkspaceController controller;
  final Size workspaceSize;
  @override
  Widget build(BuildContext context) {
    final panel = group.active;
    return RetconPanel(
      label: panel.title,
      padding: EdgeInsets.zero,
      child: Column(
        children: [
          Container(
            color: RetconColors.titleBarInactive,
            height: 34,
            child: Row(
              children: [
                Expanded(
                  child: SingleChildScrollView(
                    scrollDirection: Axis.horizontal,
                    child: Row(
                      children: [
                        for (var index = 0; index < group.panels.length; index++)
                          Padding(
                            padding: const EdgeInsets.only(left: 2),
                            child: TextButton.icon(
                              onPressed: () => unawaited(
                                controller.activateTab(group, index),
                              ),
                              icon: Icon(
                                _icon(group.panels[index].icon),
                                size: RetconIconSizes.small,
                              ),
                              label: Text(group.panels[index].title),
                            ),
                          ),
                      ],
                    ),
                  ),
                ),
                PopupMenuButton<String>(
                  tooltip: 'Panel actions',
                  onSelected: (action) {
                    if (action == 'close') {
                      unawaited(controller.close(panel));
                    }
                    if (action == 'float') {
                      unawaited(controller.float(panel, workspaceSize));
                    }
                    if (action == 'detach') {
                      unawaited(
                        controller.float(panel, workspaceSize, detached: true),
                      );
                    }
                  },
                  itemBuilder: (_) => const [
                    PopupMenuItem(value: 'float', child: Text('Float panel')),
                    PopupMenuItem(value: 'detach', child: Text('Detach panel')),
                    PopupMenuItem(value: 'close', child: Text('Close panel')),
                  ],
                ),
              ],
            ),
          ),
          Expanded(child: _PanelBody(panel: panel)),
        ],
      ),
    );
  }
}

class _FloatingView extends StatelessWidget {
  const _FloatingView({
    super.key,
    required this.floating,
    required this.controller,
  });
  final FloatingPanel floating;
  final WorkspaceController controller;
  @override
  Widget build(BuildContext context) => Positioned(
    left: floating.rect.left,
    top: floating.rect.top,
    width: floating.rect.width,
    height: floating.rect.height,
    child: Material(
      elevation: 12,
      child: RetconPanel(
        label: floating.panel.title,
        padding: EdgeInsets.zero,
        child: Column(
          children: [
            Container(
              height: 32,
              color: RetconColors.titleBarTop,
              child: Row(
                children: [
                  const SizedBox(width: RetconSpacing.sm),
                  Expanded(
                    child: Text(
                      '${floating.detached ? 'Detached: ' : ''}${floating.panel.title}',
                    ),
                  ),
                  IconButton(
                    icon: const Icon(Icons.call_merge),
                    tooltip: 'Dock panel',
                    onPressed: () => unawaited(controller.dock(floating)),
                  ),
                ],
              ),
            ),
            Expanded(child: _PanelBody(panel: floating.panel)),
          ],
        ),
      ),
    ),
  );
}

class _PanelBody extends StatelessWidget {
  const _PanelBody({required this.panel});
  final PanelDefinition panel;
  @override
  Widget build(BuildContext context) => Center(
    child: Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        Icon(_icon(panel.icon), size: 48),
        const SizedBox(height: RetconSpacing.sm),
        Text(panel.title, style: Theme.of(context).textTheme.titleLarge),
        const SizedBox(height: RetconSpacing.xs),
        if (panel.id == 'workspace')
          const Text('Retcon workspace', textAlign: TextAlign.center),
        if (panel.id == 'workspace') const SizedBox(height: RetconSpacing.xs),
        Text(_message(panel.id), textAlign: TextAlign.center),
      ],
    ),
  );
}

IconData _icon(String icon) => switch (icon) {
  'folder' => Icons.folder_open,
  'terminal' => Icons.terminal,
  'language' => Icons.language,
  _ => Icons.dashboard_customize,
};
String _message(String id) => switch (id) {
  'explorer' => 'Open a project to begin.',
  'workspace' => 'Drag and arrange your workspace panels.',
  'terminal' => 'Terminal sessions appear here.',
  'browser' => 'Browser sessions appear here.',
  _ => '',
};
