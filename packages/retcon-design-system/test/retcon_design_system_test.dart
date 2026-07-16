import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

void main() {
  test('luna dark theme uses the token palette', () {
    final theme = buildLunaDarkTheme();
    expect(theme.colorScheme.primary, RetconColors.accent);
    expect(theme.scaffoldBackgroundColor, RetconColors.desktop);
    expect(theme.useMaterial3, isTrue);
  });

  test('high contrast and reduced motion are explicit theme variants', () {
    final theme = buildLunaDarkTheme(
      highContrast: true,
      reducedMotion: true,
      largeTargets: true,
    );
    final retcon = theme.extension<RetconTheme>()!;
    expect(theme.colorScheme.primary, RetconHighContrastColors.accent);
    expect(retcon.focusColor, RetconHighContrastColors.focus);
    expect(retcon.minimumTargetSize, RetconDimensions.accessibleTarget);
    expect(retcon.motion(RetconMotion.standard), Duration.zero);
  });
}
