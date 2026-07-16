import 'package:flutter/widgets.dart';

/// Color tokens for the Luna Dark theme.
///
/// Values are the Phase 1 baseline; the definitive palette is set during the
/// Phase 6 design-system work. Nothing outside this package may hard-code
/// colors.
abstract final class RetconColors {
  /// Desktop background behind all windows.
  static const Color desktop = Color(0xFF14161F);

  /// Window chrome and panel background.
  static const Color windowBackground = Color(0xFF1F2330);

  /// Raised surface (toolbars, dialogs, property sheets).
  static const Color surface = Color(0xFF262B3B);

  /// Active title-bar gradient, top — the XP Luna blue, darkened.
  static const Color titleBarTop = Color(0xFF2A3F6F);

  /// Active title-bar gradient, bottom.
  static const Color titleBarBottom = Color(0xFF16233F);

  /// Inactive title-bar fill.
  static const Color titleBarInactive = Color(0xFF2A2E3B);

  /// Primary accent — selection, focused controls, hyperlinks.
  static const Color accent = Color(0xFF3B77BC);

  /// The Start-button green.
  static const Color startGreen = Color(0xFF3A7D3C);

  /// Primary text.
  static const Color text = Color(0xFFE4E6F0);

  /// Secondary / muted text.
  static const Color textMuted = Color(0xFF9AA2B8);

  /// Bevel highlight (top/left edges of raised elements).
  static const Color bevelLight = Color(0xFF4A5470);

  /// Bevel shadow (bottom/right edges of raised elements).
  static const Color bevelDark = Color(0xFF10131C);

  /// Error state.
  static const Color error = Color(0xFFC24038);

  /// Warning state.
  static const Color warning = Color(0xFFC79A3B);

  /// Success state.
  static const Color success = Color(0xFF4E9A51);
}

/// Typography tokens.
abstract final class RetconTypography {
  /// UI font stack — Tahoma for the XP feel, Segoe UI as fallback.
  static const List<String> uiFontFamilyFallback = ['Tahoma', 'Segoe UI'];

  /// Monospace stack for terminals, code, and diffs.
  static const List<String> monoFontFamilyFallback = [
    'Cascadia Mono',
    'Consolas',
    'Courier New',
  ];
}

/// Spacing scale, in logical pixels. XP chrome is dense; steps are small.
abstract final class RetconSpacing {
  /// 2 px — inside bevels.
  static const double xxs = 2;

  /// 4 px — between tightly related controls.
  static const double xs = 4;

  /// 8 px — default gap.
  static const double sm = 8;

  /// 12 px — between control groups.
  static const double md = 12;

  /// 16 px — panel padding.
  static const double lg = 16;

  /// 24 px — dialog padding.
  static const double xl = 24;
}
