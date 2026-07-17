import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_terminal_view/retcon_terminal_view.dart';

void main() {
  group('AnsiParser', () {
    test('parses foreground color codes', () {
      final spans = AnsiParser.parse('\u001b[31merror\u001b[0m ok');
      expect(spans.length, 2);
    });

    test('strip removes escape sequences', () {
      expect(
        AnsiParser.strip('\u001b[32mok\u001b[0m'),
        'ok',
      );
    });
  });
}
