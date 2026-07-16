import 'package:flutter/material.dart';

import 'tokens.dart';

/// Non-Material preferences consumed by Retcon components.
@immutable
class RetconTheme extends ThemeExtension<RetconTheme> {
  const RetconTheme({
    required this.highContrast,
    required this.reducedMotion,
    required this.largeTargets,
    required this.focusColor,
    required this.borderColor,
    required this.mutedTextColor,
    required this.successColor,
    required this.warningColor,
  });

  final bool highContrast;
  final bool reducedMotion;
  final bool largeTargets;
  final Color focusColor;
  final Color borderColor;
  final Color mutedTextColor;
  final Color successColor;
  final Color warningColor;

  double get minimumTargetSize => largeTargets
      ? RetconDimensions.accessibleTarget
      : RetconDimensions.compactTarget;

  Duration motion(Duration duration) =>
      reducedMotion ? Duration.zero : duration;

  static RetconTheme of(BuildContext context) =>
      Theme.of(context).extension<RetconTheme>()!;

  @override
  RetconTheme copyWith({
    bool? highContrast,
    bool? reducedMotion,
    bool? largeTargets,
    Color? focusColor,
    Color? borderColor,
    Color? mutedTextColor,
    Color? successColor,
    Color? warningColor,
  }) => RetconTheme(
    highContrast: highContrast ?? this.highContrast,
    reducedMotion: reducedMotion ?? this.reducedMotion,
    largeTargets: largeTargets ?? this.largeTargets,
    focusColor: focusColor ?? this.focusColor,
    borderColor: borderColor ?? this.borderColor,
    mutedTextColor: mutedTextColor ?? this.mutedTextColor,
    successColor: successColor ?? this.successColor,
    warningColor: warningColor ?? this.warningColor,
  );

  @override
  RetconTheme lerp(covariant RetconTheme? other, double t) {
    if (other == null) return this;
    return RetconTheme(
      highContrast: t < 0.5 ? highContrast : other.highContrast,
      reducedMotion: t < 0.5 ? reducedMotion : other.reducedMotion,
      largeTargets: t < 0.5 ? largeTargets : other.largeTargets,
      focusColor: Color.lerp(focusColor, other.focusColor, t)!,
      borderColor: Color.lerp(borderColor, other.borderColor, t)!,
      mutedTextColor: Color.lerp(mutedTextColor, other.mutedTextColor, t)!,
      successColor: Color.lerp(successColor, other.successColor, t)!,
      warningColor: Color.lerp(warningColor, other.warningColor, t)!,
    );
  }
}

/// Builds the Luna Dark theme with optional accessibility variants.
ThemeData buildLunaDarkTheme({
  bool highContrast = false,
  bool reducedMotion = false,
  bool largeTargets = false,
}) {
  final background = highContrast
      ? RetconHighContrastColors.background
      : RetconColors.desktop;
  final surface = highContrast
      ? RetconHighContrastColors.surface
      : RetconColors.windowBackground;
  final text = highContrast ? RetconHighContrastColors.text : RetconColors.text;
  final accent = highContrast
      ? RetconHighContrastColors.accent
      : RetconColors.accent;
  final error = highContrast
      ? RetconHighContrastColors.error
      : RetconColors.error;
  final border = highContrast
      ? RetconHighContrastColors.border
      : RetconColors.border;
  final focus = highContrast
      ? RetconHighContrastColors.focus
      : RetconColors.focus;
  final muted = highContrast
      ? RetconHighContrastColors.mutedText
      : RetconColors.textMuted;
  final minimumTarget = largeTargets
      ? RetconDimensions.accessibleTarget
      : RetconDimensions.compactTarget;

  final colorScheme = ColorScheme.dark(
    primary: accent,
    onPrimary: text,
    secondary: RetconColors.startGreen,
    onSecondary: text,
    surface: surface,
    onSurface: text,
    error: error,
    onError: text,
    outline: border,
  );
  final typography = TextTheme(
    displaySmall: TextStyle(fontSize: RetconTypography.display, color: text),
    headlineSmall: TextStyle(
      fontSize: RetconTypography.heading,
      fontWeight: FontWeight.w700,
      color: text,
    ),
    titleMedium: TextStyle(
      fontSize: RetconTypography.title,
      fontWeight: FontWeight.w700,
      color: text,
    ),
    bodyMedium: TextStyle(
      fontSize: RetconTypography.body,
      height: RetconTypography.lineHeight,
      color: text,
    ),
    labelLarge: TextStyle(
      fontSize: RetconTypography.label,
      fontWeight: FontWeight.w600,
      color: text,
    ),
    labelSmall: TextStyle(fontSize: RetconTypography.caption, color: muted),
  );

  OutlineInputBorder inputBorder(Color color, [double width = 1]) =>
      OutlineInputBorder(
        borderRadius: BorderRadius.circular(RetconBorders.radius),
        borderSide: BorderSide(color: color, width: width),
      );

  return ThemeData(
    useMaterial3: true,
    brightness: Brightness.dark,
    colorScheme: colorScheme,
    scaffoldBackgroundColor: background,
    canvasColor: surface,
    fontFamilyFallback: RetconTypography.uiFontFamilyFallback,
    textTheme: typography,
    primaryTextTheme: typography,
    visualDensity: largeTargets
        ? VisualDensity.standard
        : VisualDensity.compact,
    focusColor: focus,
    hoverColor: accent.withValues(alpha: 0.16),
    highlightColor: accent.withValues(alpha: 0.24),
    dividerColor: border,
    splashFactory: NoSplash.splashFactory,
    extensions: [
      RetconTheme(
        highContrast: highContrast,
        reducedMotion: reducedMotion,
        largeTargets: largeTargets,
        focusColor: focus,
        borderColor: border,
        mutedTextColor: muted,
        successColor: highContrast
            ? RetconHighContrastColors.success
            : RetconColors.success,
        warningColor: highContrast
            ? RetconHighContrastColors.warning
            : RetconColors.warning,
      ),
    ],
    inputDecorationTheme: InputDecorationTheme(
      isDense: !largeTargets,
      filled: true,
      fillColor: highContrast ? Colors.black : RetconColors.desktop,
      contentPadding: const EdgeInsets.symmetric(
        horizontal: RetconSpacing.sm,
        vertical: RetconSpacing.sm,
      ),
      enabledBorder: inputBorder(border),
      focusedBorder: inputBorder(focus, RetconBorders.focus),
      errorBorder: inputBorder(error, RetconBorders.emphasized),
      focusedErrorBorder: inputBorder(error, RetconBorders.focus),
    ),
    elevatedButtonTheme: ElevatedButtonThemeData(
      style: ElevatedButton.styleFrom(
        minimumSize: Size(64, minimumTarget),
        padding: const EdgeInsets.symmetric(horizontal: RetconSpacing.md),
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(RetconBorders.radius),
          side: BorderSide(color: border),
        ),
        backgroundColor: highContrast ? Colors.black : RetconColors.surface,
        foregroundColor: text,
        disabledBackgroundColor: RetconColors.disabled,
        disabledForegroundColor: muted,
        elevation: 0,
      ),
    ),
    tooltipTheme: TooltipThemeData(
      decoration: BoxDecoration(
        color: highContrast ? Colors.black : RetconColors.surface,
        border: Border.all(color: border),
        boxShadow: RetconShadows.menu,
      ),
      textStyle: typography.bodyMedium,
      waitDuration: reducedMotion
          ? Duration.zero
          : const Duration(milliseconds: 500),
    ),
  );
}
