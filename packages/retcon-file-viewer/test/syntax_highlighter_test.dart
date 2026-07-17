import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_file_viewer/retcon_file_viewer.dart';

void main() {
  test('SyntaxHighlighter colors rust keywords', () {
    const text = 'fn main() { let value = 42; }';
    final span = SyntaxHighlighter.highlight(
      text: text,
      language: 'rust',
      baseStyle: const TextStyle(color: Colors.white),
    );

    expect(span.children, isNotEmpty);
    expect(span.toPlainText(), text);
  });
}
