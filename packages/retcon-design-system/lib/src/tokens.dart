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

  /// Keyboard focus ring. Kept bright enough to remain visible on title bars.
  static const Color focus = Color(0xFF7EB6FF);

  /// Selected row or tab background.
  static const Color selection = Color(0xFF285A91);

  /// Disabled control fill.
  static const Color disabled = Color(0xFF353946);

  /// Divider and recessed-control border.
  static const Color border = Color(0xFF596279);
}

/// High-contrast Luna Dark colors. Status hues remain distinguishable by luminance.
abstract final class RetconHighContrastColors {
  static const Color background = Color(0xFF000000);
  static const Color surface = Color(0xFF101010);
  static const Color text = Color(0xFFFFFFFF);
  static const Color mutedText = Color(0xFFD7D7D7);
  static const Color accent = Color(0xFF73B7FF);
  static const Color focus = Color(0xFFFFFF00);
  static const Color border = Color(0xFFFFFFFF);
  static const Color error = Color(0xFFFF6B6B);
  static const Color warning = Color(0xFFFFD54F);
  static const Color success = Color(0xFF79E27D);
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

  static const double caption = 11;
  static const double body = 13;
  static const double label = 13;
  static const double title = 14;
  static const double windowTitle = 13;
  static const double heading = 20;
  static const double display = 28;
  static const double lineHeight = 1.25;
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

  /// 32 px — major sections and desktop gutters.
  static const double xxl = 32;
}

/// Border, bevel, and corner rules for Luna Dark.
abstract final class RetconBorders {
  static const double hairline = 1;
  static const double standard = 1;
  static const double emphasized = 2;
  static const double focus = 2;
  static const double radius = 2;
  static const double dialogRadius = 3;
}

/// Elevation and shadow rules. XP-style bevels carry most depth information.
abstract final class RetconShadows {
  static const List<BoxShadow> floating = [
    BoxShadow(color: Color(0x99000000), blurRadius: 14, offset: Offset(4, 6)),
  ];
  static const List<BoxShadow> menu = [
    BoxShadow(color: Color(0x88000000), blurRadius: 8, offset: Offset(2, 3)),
  ];
}

/// Motion rules. Components must use zero-duration values when reduced motion is active.
abstract final class RetconMotion {
  static const Duration instant = Duration.zero;
  static const Duration fast = Duration(milliseconds: 90);
  static const Duration standard = Duration(milliseconds: 160);
  static const Duration deliberate = Duration(milliseconds: 240);
  static const Curve curve = Curves.easeOutCubic;
}

/// Standard icon dimensions.
abstract final class RetconIconSizes {
  static const double small = 12;
  static const double standard = 16;
  static const double large = 24;
  static const double desktop = 32;
}

/// Shared desktop and panel dimensions.
abstract final class RetconDimensions {
  static const double titleBarHeight = 30;
  static const double toolbarHeight = 34;
  static const double statusBarHeight = 24;
  static const double taskbarHeight = 36;
  static const double accessibleTaskbarHeight = 48;
  static const double minimumPanelWidth = 180;
  static const double minimumPanelHeight = 120;
  static const double compactTarget = 32;
  static const double accessibleTarget = 44;
}
