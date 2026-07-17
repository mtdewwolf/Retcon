import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'controls.dart';
import '../theme.dart';
import '../tokens.dart';

/// Immutable tree node data for [RetconTreeView].
class RetconTreeNodeData {
  const RetconTreeNodeData({
    required this.id,
    required this.label,
    this.icon,
    this.children = const [],
    this.semanticsHint,
  });

  final String id;
  final String label;
  final IconData? icon;
  final List<RetconTreeNodeData> children;
  final String? semanticsHint;
}

/// Hierarchical tree with arrow-key navigation and expand/collapse semantics.
class RetconTreeView extends StatefulWidget {
  const RetconTreeView({
    required this.nodes,
    super.key,
    this.selectedId,
    this.onSelected,
    this.semanticsLabel = 'Tree view',
  });

  final List<RetconTreeNodeData> nodes;
  final String? selectedId;
  final ValueChanged<RetconTreeNodeData>? onSelected;
  final String semanticsLabel;

  @override
  State<RetconTreeView> createState() => _RetconTreeViewState();
}

class _RetconTreeViewState extends State<RetconTreeView> {
  final Set<String> _expanded = {};
  final List<RetconTreeNodeData> _visible = [];
  int _focusedIndex = 0;

  @override
  void initState() {
    super.initState();
    _rebuildVisible();
  }

  @override
  void didUpdateWidget(covariant RetconTreeView oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.nodes != widget.nodes) {
      _rebuildVisible();
    }
  }

  void _rebuildVisible() {
    _visible.clear();
    void walk(List<RetconTreeNodeData> nodes) {
      for (final node in nodes) {
        _visible.add(node);
        if (_expanded.contains(node.id) && node.children.isNotEmpty) {
          walk(node.children);
        }
      }
    }

    walk(widget.nodes);
    _focusedIndex = _focusedIndex.clamp(0, _visible.isEmpty ? 0 : _visible.length - 1);
  }

  void _toggleExpanded(RetconTreeNodeData node) {
    setState(() {
      if (_expanded.contains(node.id)) {
        _expanded.remove(node.id);
      } else {
        _expanded.add(node.id);
      }
      _rebuildVisible();
    });
  }

  void _select(RetconTreeNodeData node) {
    widget.onSelected?.call(node);
  }

  void _moveFocus(int delta) {
    if (_visible.isEmpty) return;
    setState(() {
      _focusedIndex = (_focusedIndex + delta).clamp(0, _visible.length - 1);
    });
  }

  void _handleKey(KeyDownEvent event) {
    if (_visible.isEmpty) return;
    final node = _visible[_focusedIndex];
    switch (event.logicalKey) {
      case LogicalKeyboardKey.arrowDown:
        _moveFocus(1);
      case LogicalKeyboardKey.arrowUp:
        _moveFocus(-1);
      case LogicalKeyboardKey.arrowRight:
        if (node.children.isNotEmpty && !_expanded.contains(node.id)) {
          _toggleExpanded(node);
        }
      case LogicalKeyboardKey.arrowLeft:
        if (_expanded.contains(node.id)) {
          _toggleExpanded(node);
        }
      case LogicalKeyboardKey.enter:
      case LogicalKeyboardKey.space:
        _select(node);
      case LogicalKeyboardKey.home:
        setState(() => _focusedIndex = 0);
      case LogicalKeyboardKey.end:
        setState(() => _focusedIndex = _visible.length - 1);
      default:
        break;
    }
  }

  @override
  Widget build(BuildContext context) {
    final retcon = RetconTheme.of(context);
    return Semantics(
      container: true,
      label: widget.semanticsLabel,
      child: Focus(
        onKeyEvent: (node, event) {
          if (event is KeyDownEvent) {
            _handleKey(event);
            return KeyEventResult.handled;
          }
          return KeyEventResult.ignored;
        },
        child: ListView.builder(
          shrinkWrap: true,
          itemCount: _visible.length,
          itemBuilder: (context, index) {
            final node = _visible[index];
            final depth = _depthOf(node);
            final expanded = _expanded.contains(node.id);
            final selected = widget.selectedId == node.id;
            final focused = _focusedIndex == index;

            return Semantics(
              button: true,
              selected: selected,
              expanded: node.children.isNotEmpty ? expanded : null,
              label: node.label,
              hint: node.semanticsHint,
              onTap: () {
                setState(() => _focusedIndex = index);
                _select(node);
              },
              child: InkWell(
                onTap: () {
                  setState(() => _focusedIndex = index);
                  _select(node);
                },
                child: AnimatedContainer(
                  duration: retcon.motion(RetconMotion.fast),
                  padding: EdgeInsets.only(
                    left: RetconSpacing.sm + depth * RetconSpacing.lg,
                    right: RetconSpacing.sm,
                    top: RetconSpacing.xs,
                    bottom: RetconSpacing.xs,
                  ),
                  decoration: BoxDecoration(
                    color: selected
                        ? (retcon.highContrast
                              ? retcon.focusColor.withValues(alpha: 0.25)
                              : RetconColors.selection)
                        : null,
                    border: focused
                        ? Border.all(
                            color: retcon.focusColor,
                            width: RetconBorders.focus,
                          )
                        : null,
                  ),
                  child: Row(
                    children: [
                      if (node.children.isNotEmpty)
                        RetconIconButton(
                          label: expanded ? 'Collapse ${node.label}' : 'Expand ${node.label}',
                          icon: expanded ? Icons.expand_more : Icons.chevron_right,
                          onPressed: () => _toggleExpanded(node),
                        )
                      else
                        const SizedBox(width: RetconDimensions.compactTarget),
                      if (node.icon != null) ...[
                        Icon(node.icon, size: RetconIconSizes.standard),
                        const SizedBox(width: RetconSpacing.xs),
                      ],
                      Expanded(child: Text(node.label)),
                    ],
                  ),
                ),
              ),
            );
          },
        ),
      ),
    );
  }

  int _depthOf(RetconTreeNodeData target) {
    int? walk(List<RetconTreeNodeData> nodes, int depth) {
      for (final node in nodes) {
        if (node.id == target.id) return depth;
        if (_expanded.contains(node.id)) {
          final found = walk(node.children, depth + 1);
          if (found != null) return found;
        }
      }
      return null;
    }

    return walk(widget.nodes, 0) ?? 0;
  }
}

/// Tab descriptor for [RetconTabs].
class RetconTab {
  const RetconTab({
    required this.label,
    required this.child,
    this.icon,
    this.semanticsLabel,
  });

  final String label;
  final Widget child;
  final IconData? icon;
  final String? semanticsLabel;
}

/// Keyboard-navigable tab strip with reduced-motion transitions.
class RetconTabs extends StatefulWidget {
  const RetconTabs({
    required this.tabs,
    super.key,
    this.initialIndex = 0,
    this.onChanged,
  });

  final List<RetconTab> tabs;
  final int initialIndex;
  final ValueChanged<int>? onChanged;

  @override
  State<RetconTabs> createState() => _RetconTabsState();
}

class _RetconTabsState extends State<RetconTabs> with SingleTickerProviderStateMixin {
  late TabController _controller;

  @override
  void initState() {
    super.initState();
    _controller = TabController(
      length: widget.tabs.length,
      vsync: this,
      initialIndex: widget.initialIndex.clamp(0, widget.tabs.length - 1),
    )..addListener(() {
        if (!_controller.indexIsChanging) {
          widget.onChanged?.call(_controller.index);
        }
      });
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final retcon = RetconTheme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Material(
          color: Theme.of(context).colorScheme.surface,
          child: TabBar(
            controller: _controller,
            tabs: [
              for (final tab in widget.tabs)
                Tab(
                  icon: tab.icon == null ? null : Icon(tab.icon),
                  text: tab.label,
                  height: retcon.minimumTargetSize,
                ),
            ],
          ),
        ),
        Expanded(
          child: TabBarView(
            controller: _controller,
            physics: retcon.reducedMotion
                ? const NeverScrollableScrollPhysics()
                : null,
            children: [
              for (final tab in widget.tabs)
                Semantics(
                  label: tab.semanticsLabel ?? tab.label,
                  child: tab.child,
                ),
            ],
          ),
        ),
      ],
    );
  }
}
