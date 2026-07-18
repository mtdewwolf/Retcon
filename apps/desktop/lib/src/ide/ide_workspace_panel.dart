import 'dart:async';

import 'package:flutter/material.dart';
import 'package:retcon_design_system/retcon_design_system.dart';
import 'package:retcon_file_viewer/retcon_file_viewer.dart';

import '../core_client.dart';
import 'ide_controller.dart';
import 'ide_repository.dart';

class IdeWorkspacePanel extends StatefulWidget {
  const IdeWorkspacePanel({
    required this.core,
    required this.root,
    this.events,
    super.key,
  });
  final CoreClient core;
  final String root;
  final Stream<Map<String, dynamic>>? events;

  @override
  State<IdeWorkspacePanel> createState() => _IdeWorkspacePanelState();
}

class _IdeWorkspacePanelState extends State<IdeWorkspacePanel> {
  late final CoreIdeRepository _repository;
  late final IdeController _ide;
  late final FileService _files;
  List<FileEntry> _entries = const [];
  final Map<String, List<FileEntry>> _children = {};
  final Set<String> _expanded = {};
  final Set<String> _loadingDirectories = {};
  bool _loadingFiles = true;
  Object? _fileError;
  String? _selected;
  StreamSubscription<Map<String, dynamic>>? _fileEvents;
  Timer? _fileRefresh;

  @override
  void initState() {
    super.initState();
    _repository = CoreIdeRepository(widget.core);
    _ide = IdeController(_repository)..addListener(_changed);
    _files = RpcFileService(
      (method, {params = const {}}) =>
          widget.core.request(method, params: params),
    );
    unawaited(_ide.load());
    unawaited(_loadFiles());
    _fileEvents = widget.events?.listen((event) {
      final envelope =
          (event['event'] as Map?)?.cast<String, dynamic>() ?? event;
      final kind = envelope['kind']?.toString() ?? envelope['type']?.toString();
      if (kind != 'file.changed' && kind != 'file.updated') return;
      _fileRefresh?.cancel();
      _fileRefresh = Timer(
        const Duration(milliseconds: 200),
        () => unawaited(_loadFiles()),
      );
    });
  }

  void _changed() {
    if (mounted) setState(() {});
  }

  Future<void> _loadFiles() async {
    setState(() {
      _loadingFiles = true;
      _fileError = null;
    });
    try {
      _entries = await _files.list(root: widget.root);
      _children[''] = _entries;
    } catch (error) {
      _fileError = error;
    } finally {
      if (mounted) setState(() => _loadingFiles = false);
    }
  }

  Future<void> _toggleDirectory(String path) async {
    if (_expanded.remove(path)) {
      setState(() {});
      return;
    }
    _expanded.add(path);
    if (!_children.containsKey(path)) {
      _loadingDirectories.add(path);
      setState(() {});
      try {
        _children[path] = await _files.list(root: widget.root, path: path);
      } catch (error) {
        _fileError = error;
      } finally {
        _loadingDirectories.remove(path);
      }
    }
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    _fileRefresh?.cancel();
    unawaited(_fileEvents?.cancel());
    _ide.removeListener(_changed);
    _ide.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        _IdeToolbar(controller: _ide, root: widget.root),
        const Divider(height: 1),
        Expanded(
          child: Row(
            children: [
              SizedBox(width: 260, child: _fileList()),
              const VerticalDivider(width: 1),
              Expanded(
                child: _selected == null
                    ? const Center(
                        child: Text('Choose a file to preview or edit.'),
                      )
                    : SyncedFileEditor(
                        key: ValueKey(_selected),
                        repository: _repository,
                        ide: _ide,
                        root: widget.root,
                        path: _selected!,
                        events: widget.events,
                      ),
              ),
            ],
          ),
        ),
      ],
    );
  }

  Widget _fileList() {
    if (_loadingFiles) return const Center(child: CircularProgressIndicator());
    if (_fileError != null) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(RetconSpacing.md),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              const Text(
                'Project files are unavailable.',
                textAlign: TextAlign.center,
              ),
              const SizedBox(height: RetconSpacing.sm),
              OutlinedButton(
                onPressed: _loadFiles,
                child: const Text('Try again'),
              ),
            ],
          ),
        ),
      );
    }
    return Semantics(
      label: 'Project files',
      child: ListView(
        children: [for (final entry in _entries) _entryTile(entry, 0)],
      ),
    );
  }

  Widget _entryTile(FileEntry entry, int depth) {
    if (entry.isDirectory) {
      final expanded = _expanded.contains(entry.path);
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ListTile(
            dense: true,
            contentPadding: EdgeInsets.only(
              left: RetconSpacing.sm + depth * 14,
              right: RetconSpacing.xs,
            ),
            leading: Icon(
              expanded ? Icons.expand_more : Icons.chevron_right,
              size: 18,
            ),
            title: Text(entry.name, overflow: TextOverflow.ellipsis),
            onTap: () => _toggleDirectory(entry.path),
          ),
          if (_loadingDirectories.contains(entry.path))
            const LinearProgressIndicator(minHeight: 2),
          if (expanded)
            for (final child in _children[entry.path] ?? const <FileEntry>[])
              _entryTile(child, depth + 1),
        ],
      );
    }
    return ListTile(
      dense: true,
      contentPadding: EdgeInsets.only(
        left: RetconSpacing.sm + depth * 14 + 18,
        right: RetconSpacing.xs,
      ),
      selected: entry.path == _selected,
      leading: const Icon(Icons.description_outlined, size: 18),
      title: Tooltip(
        message: 'Preview ${entry.name}',
        child: Text(entry.name, overflow: TextOverflow.ellipsis),
      ),
      onTap: () => setState(() => _selected = entry.path),
      trailing: IconButton(
        tooltip: 'Open ${entry.name} in external editor',
        icon: const Icon(Icons.open_in_new, size: 17),
        onPressed: _ide.supported
            ? () => _ide.openFile(widget.root, entry.path)
            : null,
      ),
    );
  }
}

class _IdeToolbar extends StatelessWidget {
  const _IdeToolbar({required this.controller, required this.root});
  final IdeController controller;
  final String root;

  @override
  Widget build(BuildContext context) {
    if (controller.loading && controller.ides.isEmpty) {
      return const ListTile(
        leading: SizedBox(
          width: 18,
          height: 18,
          child: CircularProgressIndicator(strokeWidth: 2),
        ),
        title: Text('Looking for installed editors…'),
      );
    }
    return Padding(
      padding: const EdgeInsets.symmetric(
        horizontal: RetconSpacing.sm,
        vertical: RetconSpacing.xs,
      ),
      child: Row(
        children: [
          const Icon(Icons.integration_instructions_outlined, size: 18),
          const SizedBox(width: RetconSpacing.sm),
          if (!controller.supported)
            const Expanded(
              child: Text(
                'No supported external editor was found. Install VS Code, Cursor, or Windsurf.',
              ),
            )
          else ...[
            DropdownButton<String>(
              value: controller.preferredIdeId,
              hint: const Text('Choose editor'),
              items: [
                for (final ide in controller.ides.where(
                  (item) => item.available,
                ))
                  DropdownMenuItem(value: ide.id, child: Text(ide.name)),
              ],
              onChanged: controller.loading ? null : controller.select,
            ),
            if (root.isNotEmpty) ...[
              const Spacer(),
              TextButton.icon(
                onPressed: controller.launching
                    ? null
                    : () => controller.openTerminal(root),
                icon: const Icon(Icons.terminal, size: 17),
                label: const Text('Terminal here'),
              ),
              TextButton.icon(
                onPressed: controller.launching
                    ? null
                    : () => controller.openWorktree(root),
                icon: const Icon(Icons.account_tree_outlined, size: 17),
                label: const Text('Open worktree'),
              ),
              FilledButton.icon(
                onPressed: controller.launching
                    ? null
                    : () => controller.openProject(root),
                icon: const Icon(Icons.open_in_new, size: 17),
                label: const Text('Open project'),
              ),
            ] else
              const Spacer(),
          ],
          if (controller.error != null) ...[
            const SizedBox(width: RetconSpacing.sm),
            Tooltip(
              message: 'The editor action could not be completed.',
              child: Icon(
                Icons.error_outline,
                color: Theme.of(context).colorScheme.error,
              ),
            ),
          ],
        ],
      ),
    );
  }
}

class SyncedFileEditor extends StatefulWidget {
  const SyncedFileEditor({
    required this.repository,
    required this.ide,
    required this.root,
    required this.path,
    this.events,
    super.key,
  });
  final SyncedFileRepository repository;
  final IdeController ide;
  final String root;
  final String path;
  final Stream<Map<String, dynamic>>? events;

  @override
  State<SyncedFileEditor> createState() => _SyncedFileEditorState();
}

class _SyncedFileEditorState extends State<SyncedFileEditor> {
  late final SyncedFileController _sync;
  late final TextEditingController _text;

  @override
  void initState() {
    super.initState();
    _text = TextEditingController();
    _sync = SyncedFileController(
      repository: widget.repository,
      root: widget.root,
      path: widget.path,
      events: widget.events,
    )..addListener(_changed);
    unawaited(_sync.load());
  }

  void _changed() {
    if (!_sync.dirty && _text.text != _sync.draft) {
      _text.value = TextEditingValue(
        text: _sync.draft,
        selection: TextSelection.collapsed(offset: _sync.draft.length),
      );
    }
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    _sync.removeListener(_changed);
    _sync.dispose();
    _text.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (_sync.loading && _sync.file == null) {
      return const Center(child: CircularProgressIndicator());
    }
    if (_sync.file == null) {
      return Center(
        child: OutlinedButton(
          onPressed: _sync.load,
          child: const Text('Try opening this file again'),
        ),
      );
    }
    final fileName = widget.path.split(RegExp(r'[/\\]')).last;
    return Column(
      children: [
        Padding(
          padding: const EdgeInsets.all(RetconSpacing.sm),
          child: Row(
            children: [
              Expanded(
                child: Text(
                  fileName,
                  style: Theme.of(context).textTheme.titleSmall,
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              if (_sync.dirty) const RetconBadge(label: 'Edited'),
              const SizedBox(width: RetconSpacing.xs),
              IconButton(
                tooltip: 'Open file in external editor',
                onPressed: widget.ide.supported
                    ? () => widget.ide.openFile(widget.root, widget.path)
                    : null,
                icon: const Icon(Icons.open_in_new),
              ),
              FilledButton.icon(
                key: const Key('synced-file-save'),
                onPressed: _sync.editable && _sync.dirty && !_sync.saving
                    ? _sync.save
                    : null,
                icon: _sync.saving
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
        if (_sync.conflict)
          MaterialBanner(
            content: const Text(
              'This file changed outside Retcon. Your draft is safe. Choose which version to continue with.',
            ),
            leading: const Icon(Icons.sync_problem),
            actions: [
              TextButton(
                key: const Key('use-external-version'),
                onPressed: _sync.useExternalVersion,
                child: const Text('Use external version'),
              ),
              TextButton(
                key: const Key('keep-draft-version'),
                onPressed: _sync.keepDraftOnLatestRevision,
                child: const Text('Keep my draft'),
              ),
            ],
          )
        else if (_sync.externalChangePending)
          const MaterialBanner(
            content: Text(
              'An external change was detected. Save will verify the file before writing.',
            ),
            actions: [SizedBox.shrink()],
          ),
        const Divider(height: 1),
        Expanded(
          child: !_sync.editable
              ? const Center(
                  child: Text(
                    'This file is read-only because it is binary or too large to edit safely.',
                  ),
                )
              : TextField(
                  key: const Key('synced-file-editor'),
                  controller: _text,
                  onChanged: _sync.edit,
                  expands: true,
                  maxLines: null,
                  style: const TextStyle(fontFamily: 'Consolas', height: 1.4),
                  decoration: const InputDecoration(
                    border: InputBorder.none,
                    contentPadding: EdgeInsets.all(RetconSpacing.sm),
                  ),
                ),
        ),
      ],
    );
  }
}

class IdeSettingsDialog extends StatefulWidget {
  const IdeSettingsDialog({
    required this.repository,
    this.projectPath,
    super.key,
  });
  final IdeRepository repository;
  final String? projectPath;
  @override
  State<IdeSettingsDialog> createState() => _IdeSettingsDialogState();
}

class _IdeSettingsDialogState extends State<IdeSettingsDialog> {
  late final IdeController _controller;
  @override
  void initState() {
    super.initState();
    _controller = IdeController(widget.repository)..addListener(_changed);
    unawaited(_controller.load());
  }

  void _changed() {
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    _controller.removeListener(_changed);
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('External editor'),
    content: SizedBox(
      width: 460,
      child: _IdeToolbar(
        controller: _controller,
        root: widget.projectPath ?? '',
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.of(context).pop(),
        child: const Text('Close'),
      ),
    ],
  );
}
