import 'dart:async';

import 'package:flutter/material.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

import 'diff_models.dart';
import 'diff_parser.dart';
import 'diff_service.dart';

/// Diff viewer with unified and side-by-side layouts and hunk actions.
class DiffViewerPanel extends StatefulWidget {
  const DiffViewerPanel({
    required this.service,
    required this.repo,
    this.path,
    this.scope = DiffScope.unstaged,
    this.viewMode = DiffViewMode.unified,
    super.key,
  });

  final DiffService service;
  final String repo;
  final String? path;
  final DiffScope scope;
  final DiffViewMode viewMode;

  @override
  State<DiffViewerPanel> createState() => _DiffViewerPanelState();
}

class _DiffViewerPanelState extends State<DiffViewerPanel> {
  final _parser = const DiffParser();
  List<DiffFile> _files = const [];
  bool _loading = false;
  Object? _error;
  String? _busyHunk;

  @override
  void initState() {
    super.initState();
    unawaited(_reload());
  }

  @override
  void didUpdateWidget(covariant DiffViewerPanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.repo != widget.repo ||
        oldWidget.path != widget.path ||
        oldWidget.scope != widget.scope) {
      unawaited(_reload());
    }
  }

  Future<void> _reload() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final diff = await widget.service.diff(
        repo: widget.repo,
        scope: widget.scope,
        path: widget.path,
      );
      _files = _parser.parse(diff);
    } catch (error) {
      _error = error;
      _files = const [];
    } finally {
      if (mounted) {
        setState(() => _loading = false);
      }
    }
  }

  Future<void> _stageHunk(DiffHunk hunk) async {
    setState(() => _busyHunk = hunk.header);
    try {
      await widget.service.stageHunk(repo: widget.repo, patch: hunk.patch);
      await _reload();
    } catch (error) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Could not stage hunk: $error')),
        );
      }
    } finally {
      if (mounted) {
        setState(() => _busyHunk = null);
      }
    }
  }

  Future<void> _discardHunk(DiffHunk hunk) async {
    setState(() => _busyHunk = hunk.header);
    try {
      await widget.service.discardHunk(repo: widget.repo, patch: hunk.patch);
      await _reload();
    } catch (error) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Could not discard hunk: $error')),
        );
      }
    } finally {
      if (mounted) {
        setState(() => _busyHunk = null);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    if (_loading) {
      return const Center(child: CircularProgressIndicator());
    }
    if (_error != null) {
      return Center(child: Text('Could not load diff: $_error'));
    }
    if (_files.isEmpty) {
      return const Center(child: Text('No changes to review.'));
    }

    return ListView.builder(
      padding: const EdgeInsets.all(RetconSpacing.sm),
      itemCount: _files.length,
      itemBuilder: (context, fileIndex) {
        final file = _files[fileIndex];
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Padding(
              padding: const EdgeInsets.symmetric(vertical: RetconSpacing.xs),
              child: Text(
                file.displayPath,
                style: Theme.of(context).textTheme.titleSmall,
              ),
            ),
            for (final hunk in file.hunks)
              _HunkCard(
                hunk: hunk,
                viewMode: widget.viewMode,
                busy: _busyHunk == hunk.header,
                onStage: () => unawaited(_stageHunk(hunk)),
                onDiscard: () => unawaited(_discardHunk(hunk)),
              ),
          ],
        );
      },
    );
  }
}

class _HunkCard extends StatelessWidget {
  const _HunkCard({
    required this.hunk,
    required this.viewMode,
    required this.busy,
    required this.onStage,
    required this.onDiscard,
  });

  final DiffHunk hunk;
  final DiffViewMode viewMode;
  final bool busy;
  final VoidCallback onStage;
  final VoidCallback onDiscard;

  @override
  Widget build(BuildContext context) {
    return Card(
      margin: const EdgeInsets.only(bottom: RetconSpacing.sm),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.all(RetconSpacing.sm),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    hunk.header,
                    style: Theme.of(context).textTheme.labelMedium,
                  ),
                ),
                TextButton(
                  onPressed: busy ? null : onStage,
                  child: const Text('Stage'),
                ),
                TextButton(
                  onPressed: busy ? null : onDiscard,
                  child: const Text('Discard'),
                ),
              ],
            ),
          ),
          if (viewMode == DiffViewMode.unified)
            _UnifiedHunkBody(hunk: hunk)
          else
            _SideBySideHunkBody(hunk: hunk),
        ],
      ),
    );
  }
}

class _UnifiedHunkBody extends StatelessWidget {
  const _UnifiedHunkBody({required this.hunk});
  final DiffHunk hunk;

  @override
  Widget build(BuildContext context) {
    return Container(
      color: RetconColors.surface,
      padding: const EdgeInsets.all(RetconSpacing.sm),
      child: SelectableText.rich(
        TextSpan(
          children: [
            for (final line in hunk.lines) _lineSpan(line),
          ],
        ),
        style: const TextStyle(fontFamily: 'monospace', fontSize: 12),
      ),
    );
  }
}

class _SideBySideHunkBody extends StatelessWidget {
  const _SideBySideHunkBody({required this.hunk});
  final DiffHunk hunk;

  @override
  Widget build(BuildContext context) {
    final rows = <_SideBySideRow>[];
    var index = 0;
    while (index < hunk.lines.length) {
      final line = hunk.lines[index];
      if (line.kind == DiffLineKind.deletion) {
        DiffLine? paired;
        final next = index + 1;
        if (next < hunk.lines.length &&
            hunk.lines[next].kind == DiffLineKind.addition) {
          paired = hunk.lines[next];
          index += 1;
        }
        rows.add(_SideBySideRow(left: line, right: paired));
      } else if (line.kind == DiffLineKind.addition) {
        rows.add(_SideBySideRow(left: null, right: line));
      } else {
        rows.add(_SideBySideRow(left: line, right: line));
      }
      index += 1;
    }

    return Container(
      color: RetconColors.surface,
      padding: const EdgeInsets.all(RetconSpacing.sm),
      child: Column(
        children: [
          for (final row in rows)
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Expanded(child: _sideCell(row.left)),
                const SizedBox(width: RetconSpacing.xs),
                Expanded(child: _sideCell(row.right)),
              ],
            ),
        ],
      ),
    );
  }

  Widget _sideCell(DiffLine? line) {
    if (line == null) {
      return const SizedBox(height: 18);
    }
    return SelectableText.rich(
      TextSpan(children: [_lineSpan(line)]),
      style: const TextStyle(fontFamily: 'monospace', fontSize: 12),
    );
  }
}

class _SideBySideRow {
  const _SideBySideRow({required this.left, required this.right});
  final DiffLine? left;
  final DiffLine? right;
}

TextSpan _lineSpan(DiffLine line) {
  final color = switch (line.kind) {
    DiffLineKind.addition => RetconColors.success,
    DiffLineKind.deletion => RetconColors.error,
    DiffLineKind.header => RetconColors.textMuted,
    DiffLineKind.context => RetconColors.text,
  };
  final prefix = switch (line.kind) {
    DiffLineKind.addition => '+',
    DiffLineKind.deletion => '-',
    DiffLineKind.header => '@',
    DiffLineKind.context => ' ',
  };
  return TextSpan(
    text: '$prefix${line.text}\n',
    style: TextStyle(color: color),
  );
}
