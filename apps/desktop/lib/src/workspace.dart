import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:retcon_diff_viewer/retcon_diff_viewer.dart';
import 'package:retcon_design_system/retcon_design_system.dart';
import 'package:retcon_file_viewer/retcon_file_viewer.dart';
import 'package:retcon_terminal_view/retcon_terminal_view.dart';

import 'approvals/approval_center.dart';
import 'browser/browser.dart';
import 'checkpoints/checkpoints.dart';
import 'conversation/conversation_panel.dart';
import 'core_client.dart';

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
  static const approvals = PanelDefinition(
    id: 'approvals',
    title: 'Approval center',
    icon: 'verified_user',
  );
  static const review = PanelDefinition(
    id: 'review',
    title: 'Diff review',
    icon: 'difference',
  );
  static const checkpoints = PanelDefinition(
    id: 'checkpoints',
    title: 'Checkpoints',
    icon: 'history',
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

  FloatingPanel copyWith({Rect? rect, bool? detached}) => FloatingPanel(
    panel: panel,
    rect: rect ?? this.rect,
    detached: detached ?? this.detached,
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

/// Persists workspace layouts through `storage.layout.*` RPC.
class RpcWorkspaceStore implements WorkspaceStore {
  RpcWorkspaceStore({
    required this.core,
    this.layoutId = 'default',
    this.workspaceId,
  });

  final CoreClient core;
  final String layoutId;
  final String? workspaceId;

  @override
  Future<WorkspaceLayout?> read() async {
    if (core.status != CoreConnectionStatus.connected) return null;
    final result = await core.request(
      'storage.layout.get',
      params: {
        'layoutId': layoutId,
        if (workspaceId != null) 'workspaceId': workspaceId,
      },
    );
    final record = result['layout'] as Map<String, dynamic>?;
    if (record == null) return null;
    final payload = record['layout'] as Map<String, dynamic>?;
    if (payload == null) return null;
    return WorkspaceLayout.fromJson(payload);
  }

  @override
  Future<void> write(WorkspaceLayout layout) async {
    if (core.status != CoreConnectionStatus.connected) return;
    await core.request(
      'storage.layout.save',
      params: {
        'layoutId': layoutId,
        'name': layoutId,
        'layout': layout.toJson(),
        if (workspaceId != null) 'workspaceId': workspaceId,
        'isActive': true,
      },
    );
  }
}

enum LayoutPreset {
  focus('Focus'),
  build('Build'),
  review('Review'),
  browser('Browser'),
  checkpoints('Checkpoints');

  const LayoutPreset(this.label);
  final String label;

  WorkspaceLayout layout() => switch (this) {
    LayoutPreset.focus => WorkspaceLayout(
      root: const TabGroup(panels: [PanelDefinition.workspace]),
    ),
    LayoutPreset.build => WorkspaceLayout(
      root: SplitGroup.horizontal(
        first: const TabGroup(panels: [PanelDefinition.explorer]),
        second: SplitGroup.vertical(
          first: const TabGroup(panels: [PanelDefinition.workspace]),
          second: const TabGroup(panels: [PanelDefinition.terminal]),
          fraction: .65,
        ),
        fraction: .22,
      ),
    ),
    LayoutPreset.review => WorkspaceLayout(
      root: SplitGroup.horizontal(
        first: const TabGroup(panels: [PanelDefinition.explorer]),
        second: SplitGroup.vertical(
          first: SplitGroup.horizontal(
            first: const TabGroup(panels: [PanelDefinition.workspace]),
            second: const TabGroup(panels: [PanelDefinition.review]),
            fraction: .55,
          ),
          second: const TabGroup(
            panels: [PanelDefinition.browser, PanelDefinition.checkpoints],
          ),
          fraction: .58,
        ),
        fraction: .22,
      ),
    ),
    LayoutPreset.browser => WorkspaceLayout(
      root: SplitGroup.horizontal(
        first: const TabGroup(panels: [PanelDefinition.browser]),
        second: const TabGroup(panels: [PanelDefinition.workspace]),
        fraction: .62,
      ),
    ),
    LayoutPreset.checkpoints => WorkspaceLayout(
      root: SplitGroup.horizontal(
        first: const TabGroup(panels: [PanelDefinition.explorer]),
        second: SplitGroup.vertical(
          first: const TabGroup(panels: [PanelDefinition.workspace]),
          second: const TabGroup(panels: [PanelDefinition.checkpoints]),
          fraction: .55,
        ),
        fraction: .22,
      ),
    ),
  };
}

class WorkspaceController extends ChangeNotifier {
  WorkspaceController({WorkspaceStore? store})
    : _store = store ?? FileWorkspaceStore();
  WorkspaceStore _store;
  String? _boundLayoutId;
  WorkspaceLayout _layout = WorkspaceLayout.initial();
  WorkspaceLayout get layout => _layout;

  void bindRpcStore(CoreClient core, {String? projectId}) {
    final layoutId = projectId == null ? 'default' : 'project-$projectId';
    if (_boundLayoutId == layoutId && _store is RpcWorkspaceStore) return;
    _boundLayoutId = layoutId;
    _store = RpcWorkspaceStore(
      core: core,
      layoutId: layoutId,
      workspaceId: projectId,
    );
    unawaited(restore());
  }

  Future<void> restore() async {
    try {
      _layout = await _store.read() ?? WorkspaceLayout.initial();
    } on Object {
      _layout = WorkspaceLayout.initial();
    }
    notifyListeners();
  }

  Future<void> reset() async {
    _layout = WorkspaceLayout.initial();
    await _save();
  }

  Future<void> applyPreset(LayoutPreset preset) async {
    _layout = preset.layout();
    await _save();
  }

  Future<void> reopenLast() async {
    if (_layout.closedPanels.isEmpty) return;
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

  Future<void> openPanel(PanelDefinition panel) async {
    if (_containsPanel(_layout.root, panel.id)) {
      await selectTab(panel.id);
      return;
    }
    _layout = WorkspaceLayout(
      root: _append(_layout.root, panel),
      floatingPanels: _layout.floatingPanels,
      closedPanels: _layout.closedPanels
          .where((item) => item.id != panel.id)
          .toList(),
    );
    await _save();
  }

  /// Activates the tab that hosts [panelId] within the docking tree.
  Future<void> selectTab(String panelId) async {
    if (!_containsPanel(_layout.root, panelId)) return;
    _layout = WorkspaceLayout(
      root: _activate(_layout.root, panelId),
      floatingPanels: _layout.floatingPanels,
      closedPanels: _layout.closedPanels,
    );
    await _save();
  }

  Future<void> close(PanelDefinition panel) async {
    _layout = WorkspaceLayout(
      root: _remove(_layout.root, panel.id),
      floatingPanels: _layout.floatingPanels,
      closedPanels: [..._layout.closedPanels, panel],
    );
    await _save();
  }

  Future<void> float(
    PanelDefinition panel,
    Size workspace, {
    bool detached = false,
  }) async {
    final safe = Rect.fromLTWH(48, 48, 360, 260).shift(
      Offset(workspace.width > 500 ? 80 : 0, workspace.height > 400 ? 40 : 0),
    );
    _layout = WorkspaceLayout(
      root: _remove(_layout.root, panel.id),
      closedPanels: _layout.closedPanels,
      floatingPanels: [
        ..._layout.floatingPanels,
        FloatingPanel(panel: panel, rect: safe, detached: detached),
      ],
    );
    await _save();
  }

  Future<void> dock(FloatingPanel floating) async {
    _layout = WorkspaceLayout(
      root: _append(_layout.root, floating.panel),
      closedPanels: _layout.closedPanels,
      floatingPanels: _layout.floatingPanels
          .where((item) => item != floating)
          .toList(),
    );
    await _save();
  }

  Future<void> moveFloating(FloatingPanel floating, Offset delta) async {
    final moved = floating.copyWith(rect: floating.rect.shift(delta));
    _layout = WorkspaceLayout(
      root: _layout.root,
      closedPanels: _layout.closedPanels,
      floatingPanels: _layout.floatingPanels
          .map((item) => item == floating ? moved : item)
          .toList(),
    );
    await _save();
  }

  Future<void> recoverMonitorLayout(Size viewport) async {
    if (_layout.floatingPanels.isEmpty) return;
    const margin = 8.0;
    final recovered = _layout.floatingPanels.map((floating) {
      var rect = floating.rect;
      if (rect.right > viewport.width - margin) {
        rect = rect.shift(Offset(viewport.width - margin - rect.right, 0));
      }
      if (rect.bottom > viewport.height - margin) {
        rect = rect.shift(Offset(0, viewport.height - margin - rect.bottom));
      }
      if (rect.left < margin) {
        rect = rect.shift(Offset(margin - rect.left, 0));
      }
      if (rect.top < margin) {
        rect = rect.shift(Offset(0, margin - rect.top));
      }
      return floating.copyWith(rect: rect);
    }).toList();
    final changed = !_panelsEqual(_layout.floatingPanels, recovered);
    if (!changed) return;
    _layout = WorkspaceLayout(
      root: _layout.root,
      closedPanels: _layout.closedPanels,
      floatingPanels: recovered,
    );
    await _save();
  }

  bool _panelsEqual(List<FloatingPanel> left, List<FloatingPanel> right) {
    if (left.length != right.length) return false;
    for (var index = 0; index < left.length; index++) {
      final a = left[index].rect;
      final b = right[index].rect;
      if (a.left != b.left ||
          a.top != b.top ||
          a.width != b.width ||
          a.height != b.height) {
        return false;
      }
    }
    return true;
  }

  Future<void> _save() async {
    notifyListeners();
    await _store.write(_layout);
  }

  bool _containsPanel(WorkspaceNode node, String id) => switch (node) {
    TabGroup group => group.panels.any((panel) => panel.id == id),
    SplitGroup split =>
      _containsPanel(split.first, id) || _containsPanel(split.second, id),
  };

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

  WorkspaceNode _activate(WorkspaceNode node, String id) => switch (node) {
    TabGroup group => () {
      final index = group.panels.indexWhere((panel) => panel.id == id);
      if (index < 0) return group;
      return TabGroup(panels: group.panels, activeIndex: index);
    }(),
    SplitGroup split => SplitGroup(
      axis: split.axis,
      first: _activate(split.first, id),
      second: _activate(split.second, id),
      fraction: split.fraction,
    ),
  };

  WorkspaceNode _remove(WorkspaceNode node, String id) => switch (node) {
    TabGroup group => () {
      final panels = group.panels.where((panel) => panel.id != id).toList();
      if (panels.isEmpty) {
        return const TabGroup(panels: [PanelDefinition.workspace]);
      }
      final activeIndex = group.activeIndex.clamp(0, panels.length - 1);
      return TabGroup(panels: panels, activeIndex: activeIndex);
    }(),
    SplitGroup split => SplitGroup(
      axis: split.axis,
      first: _remove(split.first, id),
      second: _remove(split.second, id),
      fraction: split.fraction,
    ),
  };
}

class DockingWorkspace extends StatefulWidget {
  const DockingWorkspace({
    super.key,
    required this.controller,
    this.core,
    this.browserRepository,
    this.workingDirectory,
    this.projectId,
  });
  final WorkspaceController controller;
  final CoreClient? core;
  final BrowserRepository? browserRepository;
  final String? workingDirectory;
  final String? projectId;
  @override
  State<DockingWorkspace> createState() => _DockingWorkspaceState();
}

class _DockingWorkspaceState extends State<DockingWorkspace> {
  Size? _lastViewport;

  @override
  void initState() {
    super.initState();
    unawaited(widget.controller.restore());
  }

  void _handleViewportChange(Size viewport) {
    if (_lastViewport != null && _lastViewport != viewport) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        unawaited(widget.controller.recoverMonitorLayout(viewport));
      });
    }
    _lastViewport = viewport;
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: widget.controller,
    builder: (context, _) => LayoutBuilder(
      builder: (context, bounds) {
        final viewport = bounds.biggest;
        _handleViewportChange(viewport);
        return Stack(
          children: [
            Positioned.fill(
              child: _NodeView(
                node: widget.controller.layout.root,
                controller: widget.controller,
                workspaceSize: viewport,
                core: widget.core,
                browserRepository: widget.browserRepository,
                workingDirectory: widget.workingDirectory,
                projectId: widget.projectId,
              ),
            ),
            for (final floating in widget.controller.layout.floatingPanels)
              _FloatingView(
                floating: floating,
                controller: widget.controller,
                workspaceSize: viewport,
                core: widget.core,
                browserRepository: widget.browserRepository,
                workingDirectory: widget.workingDirectory,
                projectId: widget.projectId,
              ),
          ],
        );
      },
    ),
  );
}

class _NodeView extends StatelessWidget {
  const _NodeView({
    required this.node,
    required this.controller,
    required this.workspaceSize,
    this.core,
    this.browserRepository,
    this.workingDirectory,
    this.projectId,
  });
  final WorkspaceNode node;
  final WorkspaceController controller;
  final Size workspaceSize;
  final CoreClient? core;
  final BrowserRepository? browserRepository;
  final String? workingDirectory;
  final String? projectId;
  @override
  Widget build(BuildContext context) => switch (node) {
    TabGroup group => _TabGroupView(
      group: group,
      controller: controller,
      workspaceSize: workspaceSize,
      core: core,
      browserRepository: browserRepository,
      workingDirectory: workingDirectory,
      projectId: projectId,
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
            core: core,
            browserRepository: browserRepository,
            workingDirectory: workingDirectory,
            projectId: projectId,
          ),
        ),
        const SizedBox(width: RetconSpacing.xs, height: RetconSpacing.xs),
        Expanded(
          flex: ((1 - split.fraction) * 100).round(),
          child: _NodeView(
            node: split.second,
            controller: controller,
            workspaceSize: workspaceSize,
            core: core,
            browserRepository: browserRepository,
            workingDirectory: workingDirectory,
            projectId: projectId,
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
    this.core,
    this.browserRepository,
    this.workingDirectory,
    this.projectId,
  });
  final TabGroup group;
  final WorkspaceController controller;
  final Size workspaceSize;
  final CoreClient? core;
  final BrowserRepository? browserRepository;
  final String? workingDirectory;
  final String? projectId;
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
                        for (final item in group.panels)
                          Padding(
                            padding: const EdgeInsets.only(left: 2),
                            child: TextButton.icon(
                              onPressed: () =>
                                  unawaited(controller.selectTab(item.id)),
                              style: TextButton.styleFrom(
                                foregroundColor: item.id == panel.id
                                    ? Theme.of(context).colorScheme.primary
                                    : null,
                              ),
                              icon: Icon(
                                _icon(item.icon),
                                size: RetconIconSizes.small,
                              ),
                              label: Text(item.title),
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
          Expanded(
            child: _PanelBody(
              panel: panel,
              core: core,
              browserRepository: browserRepository,
              workingDirectory: workingDirectory,
              projectId: projectId,
            ),
          ),
        ],
      ),
    );
  }
}

class _FloatingView extends StatefulWidget {
  const _FloatingView({
    required this.floating,
    required this.controller,
    required this.workspaceSize,
    this.core,
    this.browserRepository,
    this.workingDirectory,
    this.projectId,
  });
  final FloatingPanel floating;
  final WorkspaceController controller;
  final Size workspaceSize;
  final CoreClient? core;
  final BrowserRepository? browserRepository;
  final String? workingDirectory;
  final String? projectId;

  @override
  State<_FloatingView> createState() => _FloatingViewState();
}

class _FloatingViewState extends State<_FloatingView> {
  Offset? _dragOrigin;

  Future<void> _onDragEnd(DragEndDetails details) async {
    final center = widget.floating.rect.center;
    final workspace = widget.workspaceSize;
    final nearDockZone =
        center.dx < workspace.width * .25 ||
        center.dx > workspace.width * .75 ||
        center.dy > workspace.height * .75;
    if (nearDockZone) {
      await widget.controller.dock(widget.floating);
    }
    setState(() => _dragOrigin = null);
  }

  @override
  Widget build(BuildContext context) {
    final floating = widget.floating;
    return Positioned(
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
              GestureDetector(
                onPanStart: (_) =>
                    setState(() => _dragOrigin = floating.rect.topLeft),
                onPanUpdate: (details) {
                  if (_dragOrigin == null) return;
                  unawaited(
                    widget.controller.moveFloating(floating, details.delta),
                  );
                },
                onPanEnd: _onDragEnd,
                child: Container(
                  height: 32,
                  color: RetconColors.titleBarTop,
                  child: Row(
                    children: [
                      const SizedBox(width: RetconSpacing.sm),
                      const Icon(Icons.drag_indicator, size: 16),
                      const SizedBox(width: RetconSpacing.xs),
                      Expanded(
                        child: Text(
                          '${floating.detached ? 'Detached: ' : ''}${floating.panel.title}',
                        ),
                      ),
                      IconButton(
                        icon: const Icon(Icons.call_merge),
                        tooltip: 'Dock panel',
                        onPressed: () =>
                            unawaited(widget.controller.dock(floating)),
                      ),
                    ],
                  ),
                ),
              ),
              Expanded(
                child: _PanelBody(
                  panel: floating.panel,
                  core: widget.core,
                  browserRepository: widget.browserRepository,
                  workingDirectory: widget.workingDirectory,
                  projectId: widget.projectId,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _PanelBody extends StatelessWidget {
  const _PanelBody({
    required this.panel,
    this.core,
    this.browserRepository,
    this.workingDirectory,
    this.projectId,
  });
  final PanelDefinition panel;
  final CoreClient? core;
  final BrowserRepository? browserRepository;
  final String? workingDirectory;
  final String? projectId;
  @override
  Widget build(BuildContext context) {
    if (panel.id == 'workspace' && core != null) {
      return ConversationPanel(core: core!, workingDirectory: workingDirectory);
    }
    if (panel.id == 'terminal' && core != null) {
      return TerminalPanel(
        service: RpcTerminalService(
          (method, {params = const {}}) =>
              core!.request(method, params: params),
        ),
        events: core!.events,
      );
    }
    if (panel.id == 'approvals' && core != null) {
      return ApprovalCenterPanel(core: core!, projectId: projectId);
    }
    if (panel.id == 'browser' && browserRepository != null) {
      return BrowserPanel(repository: browserRepository!);
    }
    if (panel.id == 'explorer' && core != null && workingDirectory != null) {
      return FileWorkspacePanel(
        service: RpcFileService(
          (method, {params = const {}}) =>
              core!.request(method, params: params),
        ),
        root: workingDirectory!,
        events: core!.events,
      );
    }
    if (panel.id == 'review' && core != null && workingDirectory != null) {
      return DiffReviewPanel(
        service: RpcDiffService(
          (method, {params = const {}}) =>
              core!.request(method, params: params),
        ),
        repo: workingDirectory!,
      );
    }
    if (panel.id == 'checkpoints' && core != null && workingDirectory != null) {
      return CheckpointPanel(core: core!, root: workingDirectory!);
    }
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(_icon(panel.icon), size: 48),
          const SizedBox(height: RetconSpacing.sm),
          Text(panel.title, style: Theme.of(context).textTheme.titleLarge),
          const SizedBox(height: RetconSpacing.xs),
          Text(_message(panel.id), textAlign: TextAlign.center),
        ],
      ),
    );
  }
}

IconData _icon(String icon) => switch (icon) {
  'folder' => Icons.folder_open,
  'terminal' => Icons.terminal,
  'language' => Icons.language,
  'verified_user' => Icons.verified_user,
  'difference' => Icons.difference,
  'history' => Icons.history,
  _ => Icons.dashboard_customize,
};
String _message(String id) => switch (id) {
  'explorer' => 'Open a project to begin.',
  'workspace' => 'Agent conversation appears here when Core is connected.',
  'terminal' => 'Terminal sessions appear here.',
  'browser' => 'Connect to Retcon Core to start the browser service.',
  'approvals' => 'Pending approvals appear here when Core is connected.',
  'review' => 'Diff review appears here when a project is open.',
  'checkpoints' => 'Checkpoint history appears here when a project is open.',
  _ => '',
};
