import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

void main() {
  test('luna dark theme uses the token palette', () {
    final theme = buildLunaDarkTheme();
    expect(theme.colorScheme.primary, RetconColors.accent);
    expect(theme.scaffoldBackgroundColor, RetconColors.desktop);
    expect(theme.useMaterial3, isTrue);
  });
}
