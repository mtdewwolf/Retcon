import 'diff_models.dart';

/// Parse unified diff text into structured files and hunks.
class DiffParser {
  const DiffParser();

  List<DiffFile> parse(String diff) {
    if (diff.trim().isEmpty) {
      return const [];
    }

    final files = <DiffFile>[];
    final lines = diff.split('\n');
    var index = 0;

    while (index < lines.length) {
      if (!lines[index].startsWith('diff --git')) {
        index += 1;
        continue;
      }

      final header = lines[index];
      index += 1;
      var oldPath = '';
      var newPath = '';
      final hunks = <DiffHunk>[];

      while (index < lines.length && !lines[index].startsWith('diff --git')) {
        final line = lines[index];
        if (line.startsWith('--- ')) {
          oldPath = _stripPath(line.substring(4));
        } else if (line.startsWith('+++ ')) {
          newPath = _stripPath(line.substring(4));
        } else if (line.startsWith('@@')) {
          final hunkLines = <String>[line];
          index += 1;
          while (index < lines.length &&
              !lines[index].startsWith('@@') &&
              !lines[index].startsWith('diff --git')) {
            hunkLines.add(lines[index]);
            index += 1;
          }
          hunks.add(_parseHunk(hunkLines));
          continue;
        }
        index += 1;
      }

      if (oldPath.isEmpty && newPath.isEmpty) {
        final match = RegExp(r'diff --git a/(.+) b/(.+)').firstMatch(header);
        oldPath = match?.group(1) ?? 'unknown';
        newPath = match?.group(2) ?? oldPath;
      }

      files.add(
        DiffFile(oldPath: oldPath, newPath: newPath, hunks: hunks),
      );
    }

    return files;
  }

  DiffHunk _parseHunk(List<String> hunkLines) {
    final header = hunkLines.first;
    final match = RegExp(
      r'@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@',
    ).firstMatch(header);
    var oldLine = int.tryParse(match?.group(1) ?? '') ?? 0;
    var newLine = int.tryParse(match?.group(2) ?? '') ?? 0;
    final parsedLines = <DiffLine>[];

    for (final line in hunkLines.skip(1)) {
      if (line.startsWith('\\')) {
        parsedLines.add(
          DiffLine(
            kind: DiffLineKind.context,
            text: line,
            oldLineNo: null,
            newLineNo: null,
          ),
        );
        continue;
      }
      final prefix = line.isEmpty ? ' ' : line[0];
      final text = line.isEmpty ? '' : line.substring(1);
      switch (prefix) {
        case '+':
          parsedLines.add(
            DiffLine(
              kind: DiffLineKind.addition,
              text: text,
              oldLineNo: null,
              newLineNo: newLine,
            ),
          );
          newLine += 1;
        case '-':
          parsedLines.add(
            DiffLine(
              kind: DiffLineKind.deletion,
              text: text,
              oldLineNo: oldLine,
              newLineNo: null,
            ),
          );
          oldLine += 1;
        default:
          parsedLines.add(
            DiffLine(
              kind: DiffLineKind.context,
              text: text,
              oldLineNo: oldLine,
              newLineNo: newLine,
            ),
          );
          oldLine += 1;
          newLine += 1;
      }
    }

    return DiffHunk(
      header: header,
      lines: parsedLines,
      patch: '${hunkLines.join('\n')}\n',
    );
  }

  String _stripPath(String raw) {
    final trimmed = raw.trim();
    if (trimmed.startsWith('a/') || trimmed.startsWith('b/')) {
      return trimmed.substring(2);
    }
    return trimmed;
  }
}
