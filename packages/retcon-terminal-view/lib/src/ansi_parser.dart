import 'package:flutter/material.dart';

/// Minimal ANSI SGR parser for common foreground colors and reset.
class AnsiParser {
  static List<InlineSpan> parse(String text, {TextStyle? baseStyle}) {
    final style = baseStyle ??
        const TextStyle(
          fontFamily: 'Consolas',
          fontFamilyFallback: ['Courier New', 'monospace'],
          fontSize: 13,
          height: 1.2,
        );
    final spans = <InlineSpan>[];
    var current = style;
    final buffer = StringBuffer();
    var index = 0;

    void flush() {
      if (buffer.isEmpty) return;
      spans.add(TextSpan(text: buffer.toString(), style: current));
      buffer.clear();
    }

    while (index < text.length) {
      if (text[index] == '\u001b' && index + 1 < text.length && text[index + 1] == '[') {
        flush();
        final end = text.indexOf('m', index);
        if (end == -1) {
          buffer.write(text[index]);
          index++;
          continue;
        }
        final code = text.substring(index + 2, end);
        current = _applyCode(code, style, current);
        index = end + 1;
        continue;
      }
      buffer.write(text[index]);
      index++;
    }
    flush();
    if (spans.isEmpty) {
      spans.add(TextSpan(text: '', style: style));
    }
    return spans;
  }

  static TextStyle _applyCode(String code, TextStyle base, TextStyle current) {
    if (code == '0') return base;
    return switch (code) {
      '1' => current.copyWith(fontWeight: FontWeight.bold),
      '30' => current.copyWith(color: const Color(0xFFCCCCCC)),
      '31' => current.copyWith(color: const Color(0xFFE06C75)),
      '32' => current.copyWith(color: const Color(0xFF98C379)),
      '33' => current.copyWith(color: const Color(0xFFE5C07B)),
      '34' => current.copyWith(color: const Color(0xFF61AFEF)),
      '35' => current.copyWith(color: const Color(0xFFC678DD)),
      '36' => current.copyWith(color: const Color(0xFF56B6C2)),
      '37' => current.copyWith(color: const Color(0xFFABB2BF)),
      '90' => current.copyWith(color: const Color(0xFF5C6370)),
      '91' => current.copyWith(color: const Color(0xFFBE5046)),
      '92' => current.copyWith(color: const Color(0xFF7FA85F)),
      '93' => current.copyWith(color: const Color(0xFFD19A66)),
      '94' => current.copyWith(color: const Color(0xFF4A83C4)),
      '95' => current.copyWith(color: const Color(0xFFA868C8)),
      '96' => current.copyWith(color: const Color(0xFF4AA5AA)),
      _ => current,
    };
  }

  static String strip(String text) {
    final buffer = StringBuffer();
    var index = 0;
    while (index < text.length) {
      if (text[index] == '\u001b' && index + 1 < text.length && text[index + 1] == '[') {
        final end = text.indexOf('m', index);
        if (end == -1) {
          buffer.write(text[index]);
          index++;
          continue;
        }
        index = end + 1;
        continue;
      }
      buffer.write(text[index]);
      index++;
    }
    return buffer.toString();
  }
}
