import 'package:flutter/material.dart';

import 'tokens.dart';

/// Builds the base Luna Dark [ThemeData].
///
/// Phase 1 provides only enough theming for the placeholder shell; the full
/// component theme arrives with the Phase 6 design system.
ThemeData buildLunaDarkTheme() {
  final colorScheme = ColorScheme.dark(
    primary: RetconColors.accent,
    surface: RetconColors.windowBackground,
    error: RetconColors.error,
    onPrimary: RetconColors.text,
    onSurface: RetconColors.text,
  );

  return ThemeData(
    useMaterial3: true,
    colorScheme: colorScheme,
    scaffoldBackgroundColor: RetconColors.desktop,
    canvasColor: RetconColors.windowBackground,
    fontFamilyFallback: RetconTypography.uiFontFamilyFallback,
    visualDensity: VisualDensity.compact,
  );
}
