/// Parsed diff structures for the Retcon diff viewer.
class DiffFile {
  const DiffFile({
    required this.oldPath,
    required this.newPath,
    required this.hunks,
  });

  final String oldPath;
  final String newPath;
  final List<DiffHunk> hunks;

  String get displayPath => newPath == '/dev/null' ? oldPath : newPath;
}

/// One contiguous hunk within a unified diff.
class DiffHunk {
  const DiffHunk({
    required this.header,
    required this.lines,
    required this.patch,
  });

  final String header;
  final List<DiffLine> lines;
  final String patch;
}

/// A single line in a diff hunk.
class DiffLine {
  const DiffLine({
    required this.kind,
    required this.text,
    required this.oldLineNo,
    required this.newLineNo,
  });

  final DiffLineKind kind;
  final String text;
  final int? oldLineNo;
  final int? newLineNo;
}

enum DiffLineKind { context, addition, deletion, header }

enum DiffViewMode { unified, sideBySide }

enum DiffScope { unstaged, staged, all }
