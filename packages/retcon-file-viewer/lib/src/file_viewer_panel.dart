import 'dart:async';

import 'package:flutter/material.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

import 'file_explorer_panel.dart';
import 'file_models.dart';
import 'file_service.dart';
import 'syntax_highlighter.dart';

/// Default read limit for the viewer (512 KiB).
const defaultReadLimit = 512 * 1024;

/// Lightweight file viewer with syntax highlighting and save support.
class FileViewerPanel extends StatefulWidget {
  const FileViewerPanel({
    required this.service,
    required this.root,
    required this.path,
    this.readLimit = defaultReadLimit,
    super.key,
  });

  final FileService service;
  final String root;
  final String path;
  final int readLimit;

  @override
  State<FileViewerPanel> createState() => _FileViewerPanelState();
}

class _FileViewerPanelState extends State<FileViewerPanel> {
  final _controller = TextEditingController();
  FileReadResult? _loaded;
  bool _loading = false;
  bool _dirty = false;
  bool _saving = false;
  Object? _error;

  @override
  void initState() {
    super.initState();
    _controller.addListener(_handleEdited);
    unawaited(_load());
  }

  @override
  void didUpdateWidget(covariant FileViewerPanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.path != widget.path || oldWidget.root != widget.root) {
      unawaited(_load());
    }
  }

  void _handleEdited() {
    if (_loaded == null) return;
    final dirty = _controller.text != (_loaded!.content ?? '');
    if (dirty != _dirty) {
      setState(() => _dirty = dirty);
    }
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
      _error = null;
      _dirty = false;
    });
    try {
      final loaded = await widget.service.read(
        root: widget.root,
        path: widget.path,
        limit: widget.readLimit,
      );
      _loaded = loaded;
      _controller.text = loaded.content ?? '';
    } catch (error) {
      _error = error;
    } finally {
      if (mounted) {
        setState(() => _loading = false);
      }
    }
  }

  Future<void> _save() async {
    if (_loaded?.binary == true || _loaded?.truncated == true) return;
    setState(() => _saving = true);
    try {
      await widget.service.write(
        root: widget.root,
        path: widget.path,
        content: _controller.text,
      );
      _dirty = false;
      await _load();
    } catch (error) {
      _error = error;
    } finally {
      if (mounted) {
        setState(() => _saving = false);
      }
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final loaded = _loaded;
    if (_loading && loaded == null) {
      return const Center(child: CircularProgressIndicator());
    }
    if (_error != null) {
      return Center(child: Text('Could not open file.\n$_error'));
    }
    if (loaded == null) {
      return const Center(child: Text('Select a file to preview.'));
    }

    final fileName = widget.path.split(RegExp(r'[/\\]')).last;
    final theme = Theme.of(context);
    final mono = theme.textTheme.bodyMedium?.copyWith(
      fontFamily: 'Consolas',
      height: 1.4,
    );

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.all(RetconSpacing.sm),
          child: Row(
            children: [
              Expanded(
                child: Text(
                  fileName,
                  style: theme.textTheme.titleSmall,
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              if (loaded.truncated)
                const RetconBadge(
                  label: 'Large file',
                  status: RetconStatus.warning,
                ),
              if (_dirty)
                const RetconBadge(
                  label: 'Edited',
                  status: RetconStatus.neutral,
                ),
              const SizedBox(width: RetconSpacing.sm),
              FilledButton.icon(
                onPressed:
                    _saving || loaded.binary || loaded.truncated || !_dirty
                    ? null
                    : _save,
                icon: _saving
                    ? const SizedBox(
                        width: 14,
                        height: 14,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      )
                    : const Icon(Icons.save, size: 16),
                label: const Text('Save'),
              ),
            ],
          ),
        ),
        const Divider(height: 1),
        Expanded(
          child: loaded.binary
              ? _BinaryNotice(size: loaded.size)
              : loaded.truncated
              ? _TruncatedNotice(
                  size: loaded.size,
                  preview: loaded.content ?? '',
                  language: loaded.language,
                  style: mono ?? const TextStyle(),
                )
              : Padding(
                  padding: const EdgeInsets.all(RetconSpacing.sm),
                  child: TextField(
                    controller: _controller,
                    maxLines: null,
                    expands: true,
                    style: mono,
                    decoration: const InputDecoration(
                      border: InputBorder.none,
                      isCollapsed: true,
                    ),
                  ),
                ),
        ),
        if (!loaded.binary && !loaded.truncated)
          Container(
            padding: const EdgeInsets.all(RetconSpacing.sm),
            decoration: BoxDecoration(
              border: Border(top: BorderSide(color: theme.dividerColor)),
            ),
            child: SingleChildScrollView(
              scrollDirection: Axis.horizontal,
              child: RichText(
                text: SyntaxHighlighter.highlight(
                  text: _controller.text,
                  language: loaded.language,
                  baseStyle: mono ?? const TextStyle(fontSize: 12),
                ),
              ),
            ),
          ),
      ],
    );
  }
}

class _BinaryNotice extends StatelessWidget {
  const _BinaryNotice({required this.size});
  final int size;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Text(
        'Binary file ($size bytes).\nOpen in an external editor to inspect.',
        textAlign: TextAlign.center,
      ),
    );
  }
}

class _TruncatedNotice extends StatelessWidget {
  const _TruncatedNotice({
    required this.size,
    required this.preview,
    required this.language,
    required this.style,
  });

  final int size;
  final String preview;
  final String language;
  final TextStyle style;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.all(RetconSpacing.md),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            'File is $size bytes. Showing a read-only preview of the first ${preview.length} bytes.',
          ),
          const SizedBox(height: RetconSpacing.sm),
          Expanded(
            child: SingleChildScrollView(
              child: RichText(
                text: SyntaxHighlighter.highlight(
                  text: preview,
                  language: language,
                  baseStyle: style,
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// Combined explorer + viewer surface for the workspace panel.
class FileWorkspacePanel extends StatefulWidget {
  const FileWorkspacePanel({
    required this.service,
    required this.root,
    this.events,
    this.onOpenExternal,
    super.key,
  });

  final FileService service;
  final String root;
  final Stream<Map<String, dynamic>>? events;
  final Future<void> Function(String path)? onOpenExternal;

  @override
  State<FileWorkspacePanel> createState() => _FileWorkspacePanelState();
}

class _FileWorkspacePanelState extends State<FileWorkspacePanel> {
  String? _selectedPath;

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        Expanded(
          flex: 2,
          child: FileExplorerPanel(
            service: widget.service,
            root: widget.root,
            events: widget.events,
            onOpenExternal: widget.onOpenExternal,
            onFileSelected: (path) => setState(() => _selectedPath = path),
          ),
        ),
        const VerticalDivider(width: 1),
        Expanded(
          flex: 3,
          child: _selectedPath == null
              ? const Center(child: Text('Select a file to preview.'))
              : FileViewerPanel(
                  service: widget.service,
                  root: widget.root,
                  path: _selectedPath!,
                ),
        ),
      ],
    );
  }
}
