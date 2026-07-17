import 'package:flutter/material.dart';

/// Lightweight syntax highlighting for common source languages.
class SyntaxHighlighter {
  const SyntaxHighlighter._();

  static TextSpan highlight({
    required String text,
    required String language,
    required TextStyle baseStyle,
  }) {
    final keywordStyle = baseStyle.copyWith(
      color: const Color(0xFF82AAFF),
      fontWeight: FontWeight.w600,
    );
    final stringStyle = baseStyle.copyWith(color: const Color(0xFFC3E88D));
    final commentStyle = baseStyle.copyWith(
      color: const Color(0xFF697098),
      fontStyle: FontStyle.italic,
    );
    final numberStyle = baseStyle.copyWith(color: const Color(0xFFF78C6C));

    final keywords = _keywordsFor(language);
    final spans = <InlineSpan>[];
    final buffer = StringBuffer();
    var index = 0;

    void flush({TextStyle? style}) {
      if (buffer.isEmpty) return;
      spans.add(TextSpan(text: buffer.toString(), style: style ?? baseStyle));
      buffer.clear();
    }

    while (index < text.length) {
      final remaining = text.substring(index);
      if (remaining.startsWith('//') ||
          (language == 'python' && remaining.startsWith('#'))) {
        flush();
        final end = text.indexOf('\n', index);
        final slice = end == -1 ? text.substring(index) : text.substring(index, end);
        spans.add(TextSpan(text: slice, style: commentStyle));
        index += slice.length;
        continue;
      }
      if (remaining.startsWith('"') || remaining.startsWith("'")) {
        flush();
        final quote = remaining[0];
        var cursor = 1;
        while (cursor < remaining.length) {
          if (remaining[cursor] == '\\') {
            cursor += 2;
            continue;
          }
          if (remaining[cursor] == quote) {
            cursor += 1;
            break;
          }
          cursor += 1;
        }
        spans.add(
          TextSpan(
            text: remaining.substring(0, cursor),
            style: stringStyle,
          ),
        );
        index += cursor;
        continue;
      }
      final match = RegExp(r'^[A-Za-z_][A-Za-z0-9_]*').firstMatch(remaining);
      if (match != null) {
        final word = match.group(0)!;
        flush(style: keywords.contains(word) ? keywordStyle : baseStyle);
        spans.add(
          TextSpan(
            text: word,
            style: keywords.contains(word) ? keywordStyle : baseStyle,
          ),
        );
        index += word.length;
        continue;
      }
      final number = RegExp(r'^\d+(?:\.\d+)?').firstMatch(remaining);
      if (number != null) {
        flush();
        spans.add(TextSpan(text: number.group(0), style: numberStyle));
        index += number.group(0)!.length;
        continue;
      }
      buffer.write(text[index]);
      index += 1;
    }
    flush();
    return TextSpan(style: baseStyle, children: spans);
  }

  static Set<String> _keywordsFor(String language) => switch (language) {
    'rust' => {
      'fn',
      'let',
      'mut',
      'pub',
      'struct',
      'enum',
      'impl',
      'use',
      'match',
      'return',
      'async',
      'await',
      'if',
      'else',
      'true',
      'false',
    },
    'dart' => {
      'class',
      'extends',
      'implements',
      'import',
      'return',
      'final',
      'const',
      'var',
      'void',
      'async',
      'await',
      'if',
      'else',
      'true',
      'false',
    },
    'javascript' || 'typescript' => {
      'function',
      'const',
      'let',
      'var',
      'return',
      'import',
      'export',
      'async',
      'await',
      'if',
      'else',
      'true',
      'false',
    },
    'python' => {
      'def',
      'class',
      'return',
      'import',
      'from',
      'async',
      'await',
      'if',
      'else',
      'True',
      'False',
    },
    _ => const {},
  };
}
