import 'package:flutter/material.dart';

import '../theme.dart';
import '../tokens.dart';

/// Recessed or raised content surface used to compose windows and dialogs.
class RetconPanel extends StatelessWidget {
  const RetconPanel({
    required this.child,
    super.key,
    this.label,
    this.padding = const EdgeInsets.all(RetconSpacing.md),
    this.recessed = false,
  });

  final Widget child;
  final String? label;
  final EdgeInsetsGeometry padding;
  final bool recessed;

  @override
  Widget build(BuildContext context) {
    final retcon = RetconTheme.of(context);
    final light = retcon.highContrast
        ? retcon.borderColor
        : RetconColors.bevelLight;
    final dark = retcon.highContrast
        ? retcon.borderColor
        : RetconColors.bevelDark;
    final topLeft = recessed ? dark : light;
    final bottomRight = recessed ? light : dark;
    return Semantics(
      container: true,
      label: label,
      child: Container(
        constraints: const BoxConstraints(
          minWidth: RetconDimensions.minimumPanelWidth,
          minHeight: RetconDimensions.minimumPanelHeight,
        ),
        padding: padding,
        decoration: BoxDecoration(
          color: Theme.of(context).colorScheme.surface,
          border: Border(
            top: BorderSide(color: topLeft),
            left: BorderSide(color: topLeft),
            right: BorderSide(
              color: bottomRight,
              width: RetconBorders.emphasized,
            ),
            bottom: BorderSide(
              color: bottomRight,
              width: RetconBorders.emphasized,
            ),
          ),
        ),
        child: child,
      ),
    );
  }
}

enum RetconStatus { neutral, success, warning, error }

/// Status badge that pairs every status color with an icon and text label.
class RetconBadge extends StatelessWidget {
  const RetconBadge({
    required this.label,
    super.key,
    this.status = RetconStatus.neutral,
  });

  final String label;
  final RetconStatus status;

  @override
  Widget build(BuildContext context) {
    final retcon = RetconTheme.of(context);
    final (color, icon) = switch (status) {
      RetconStatus.success => (retcon.successColor, Icons.check_circle),
      RetconStatus.warning => (retcon.warningColor, Icons.warning_amber),
      RetconStatus.error => (Theme.of(context).colorScheme.error, Icons.error),
      RetconStatus.neutral => (retcon.mutedTextColor, Icons.info_outline),
    };
    return Semantics(
      label: '${status.name}: $label',
      excludeSemantics: true,
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: color.withValues(alpha: 0.14),
          border: Border.all(color: color),
          borderRadius: BorderRadius.circular(RetconBorders.radius),
        ),
        child: Padding(
          padding: const EdgeInsets.symmetric(
            horizontal: RetconSpacing.sm,
            vertical: RetconSpacing.xs,
          ),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(icon, size: RetconIconSizes.small, color: color),
              const SizedBox(width: RetconSpacing.xs),
              Text(label),
            ],
          ),
        ),
      ),
    );
  }
}

/// Linear progress with determinate and indeterminate semantics.
class RetconProgressBar extends StatelessWidget {
  const RetconProgressBar({super.key, this.value, this.label});

  final double? value;
  final String? label;

  @override
  Widget build(BuildContext context) => Semantics(
    label: label,
    value: value == null ? 'In progress' : '${(value! * 100).round()}%',
    child: SizedBox(
      height: RetconSpacing.sm,
      child: LinearProgressIndicator(value: value),
    ),
  );
}
