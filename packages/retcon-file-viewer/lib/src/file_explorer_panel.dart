import 'dart:async';

import 'package:flutter/material.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

import 'file_models.dart';
import 'file_service.dart';

/// Lazy tree controller for the project explorer.
class FileExplorerController extends ChangeNotifier {
  FileExplorerController({
    required FileService service,
    required String root,
    Stream<Map<String, dynamic>>? events,
    Future<void> Function(String path)? onOpenExternal,
  }) : _service = service,
       _root = root,
       _onOpenExternal = onOpenExternal {
    _events = events?.listen(_handleEvent);
    unawaited(refresh());
  }

  final FileService _service;
  final String _root;
  final Future<void> Function(String path)? _onOpenExternal;
  StreamSubscription<Map<String, dynamic>>? _events;
  FileWatchHandle? _watch;

  final Map<String, List<FileEntry>> _children = {};
  final Set<String> _expanded = {};
  final Set<String> _loading = {};
  String? _selectedPath;
  Object? _error;

  String get root => _root;
  String? get selectedPath => _selectedPath;
  Object? get error => _error;
  bool get canOpenExternal => _onOpenExternal != null;
  Iterable<String> get expandedPaths => _expanded;

  List<FileEntry> entriesFor(String? path) => _children[path ?? ''] ?? const [];

  bool isExpanded(String path) => _expanded.contains(path);
  bool isLoading(String path) => _loading.contains(path);

  Future<void> refresh() async {
    _error = null;
    await _loadDirectory(null);
    await _ensureWatch();
    notifyListeners();
  }

  Future<void> toggle(String path) async {
    if (_expanded.contains(path)) {
      _expanded.remove(path);
    } else {
      _expanded.add(path);
      if (!_children.containsKey(path)) {
        await _loadDirectory(path);
      }
    }
    notifyListeners();
  }

  void select(String path) {
    _selectedPath = path;
    notifyListeners();
  }

  Future<void> openExternal(String path) async {
    await _onOpenExternal?.call(path);
  }

  Future<void> disposeController() async {
    await _events?.cancel();
    final watch = _watch;
    if (watch != null) {
      await _service.unwatch(watchId: watch.watchId);
    }
  }

  Future<void> _ensureWatch() async {
    if (_watch != null) return;
    _watch = await _service.watch(root: _root, watchId: 'explorer-$_root');
  }

  Future<void> _loadDirectory(String? path) async {
    final key = path ?? '';
    _loading.add(key);
    notifyListeners();
    try {
      final entries = await _service.list(root: _root, path: path);
      _children[key] = entries;
      _error = null;
    } catch (error) {
      _error = error;
    } finally {
      _loading.remove(key);
      notifyListeners();
    }
  }

  void _handleEvent(Map<String, dynamic> event) {
    final kind = event['kind']?.toString();
    if (kind != 'file.changed') return;
    final payload =
        (event['payload'] as Map?)?.cast<String, dynamic>() ?? event;
    final change = FileChangeEvent.fromPayload(payload);
    if (change.watchId != _watch?.watchId) return;
    final parent = _parentDirectoryKey(change.path);
    unawaited(_loadDirectory(parent));
  }

  String? _parentDirectoryKey(String path) {
    if (path == _root) return null;
    final parent = _parentPath(path);
    if (parent == null || parent == _root) return null;
    return parent;
  }

  String? _parentPath(String path) {
    final index = path.lastIndexOf(RegExp(r'[/\\]'));
    if (index <= 0) return null;
    return path.substring(0, index);
  }
}

/// Explorer panel with lazy directory tree and git status badges.
class FileExplorerPanel extends StatefulWidget {
  const FileExplorerPanel({
    required this.service,
    required this.root,
    this.events,
    this.onFileSelected,
    this.onOpenExternal,
    super.key,
  });

  final FileService service;
  final String root;
  final Stream<Map<String, dynamic>>? events;
  final ValueChanged<String>? onFileSelected;
  final Future<void> Function(String path)? onOpenExternal;

  @override
  State<FileExplorerPanel> createState() => _FileExplorerPanelState();
}

class _FileExplorerPanelState extends State<FileExplorerPanel> {
  late final FileExplorerController _controller;

  @override
  void initState() {
    super.initState();
    _controller = FileExplorerController(
      service: widget.service,
      root: widget.root,
      events: widget.events,
      onOpenExternal: widget.onOpenExternal,
    );
    _controller.addListener(_notifySelection);
  }

  @override
  void didUpdateWidget(covariant FileExplorerPanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.root != widget.root) {
      unawaited(_controller.disposeController());
      _controller.dispose();
      _controller = FileExplorerController(
        service: widget.service,
        root: widget.root,
        events: widget.events,
        onOpenExternal: widget.onOpenExternal,
      );
      _controller.addListener(_notifySelection);
    }
  }

  void _notifySelection() {
    final selected = _controller.selectedPath;
    if (selected != null) {
      widget.onFileSelected?.call(selected);
    }
    setState(() {});
  }

  @override
  void dispose() {
    _controller.removeListener(_notifySelection);
    unawaited(_controller.disposeController());
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (_controller.error != null) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(RetconSpacing.md),
          child: Text('Could not load project files.\n$_controller.error'),
        ),
      );
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.all(RetconSpacing.sm),
          child: Text(
            widget.root,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: Theme.of(context).textTheme.labelLarge,
          ),
        ),
        const Divider(height: 1),
        Expanded(
          child: ListView(
            children: [
              for (final entry in _controller.entriesFor(null))
                _ExplorerTile(controller: _controller, entry: entry, depth: 0),
            ],
          ),
        ),
      ],
    );
  }
}

class _ExplorerTile extends StatelessWidget {
  const _ExplorerTile({
    required this.controller,
    required this.entry,
    required this.depth,
  });

  final FileExplorerController controller;
  final FileEntry entry;
  final int depth;

  @override
  Widget build(BuildContext context) {
    if (entry.isDirectory) {
      final expanded = controller.isExpanded(entry.path);
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ListTile(
            dense: true,
            contentPadding: EdgeInsets.only(
              left: RetconSpacing.sm + depth * 16,
              right: RetconSpacing.sm,
            ),
            leading: Icon(
              expanded ? Icons.expand_more : Icons.chevron_right,
              size: 18,
            ),
            title: Text(entry.name),
            trailing: _GitBadge(status: entry.gitStatus),
            onTap: () => controller.toggle(entry.path),
          ),
          if (expanded)
            for (final child in controller.entriesFor(entry.path))
              _ExplorerTile(
                controller: controller,
                entry: child,
                depth: depth + 1,
              ),
          if (expanded && controller.isLoading(entry.path))
            Padding(
              padding: EdgeInsets.only(left: RetconSpacing.lg + depth * 16),
              child: const LinearProgressIndicator(minHeight: 2),
            ),
        ],
      );
    }

    return ListTile(
      dense: true,
      contentPadding: EdgeInsets.only(
        left: RetconSpacing.sm + depth * 16 + 18,
        right: RetconSpacing.sm,
      ),
      leading: const Icon(Icons.description_outlined, size: 18),
      title: Text(entry.name),
      trailing: _GitBadge(status: entry.gitStatus),
      selected: controller.selectedPath == entry.path,
      onTap: () => controller.select(entry.path),
      onLongPress: controller.canOpenExternal
          ? () => controller.openExternal(entry.path)
          : null,
    );
  }
}

class _GitBadge extends StatelessWidget {
  const _GitBadge({this.status});
  final String? status;

  @override
  Widget build(BuildContext context) {
    if (status == null) return const SizedBox.shrink();
    final color = switch (status) {
      'modified' => Colors.orangeAccent,
      'added' => Colors.lightGreenAccent,
      'deleted' => Colors.redAccent,
      _ => Theme.of(context).colorScheme.primary,
    };
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
      decoration: BoxDecoration(
        border: Border.all(color: color.withValues(alpha: 0.6)),
        borderRadius: BorderRadius.circular(999),
      ),
      child: Text(
        status!.substring(0, 1).toUpperCase(),
        style: Theme.of(context).textTheme.labelSmall?.copyWith(color: color),
      ),
    );
  }
}
