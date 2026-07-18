import 'dart:async';

import 'package:flutter/material.dart';

import '../theme.dart';
import '../tokens.dart';
import 'surfaces.dart';

/// XP-inspired desktop taskbar surface shared by the application shell.
class RetconTaskbar extends StatelessWidget {
  const RetconTaskbar({required this.child, super.key});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    final retcon = RetconTheme.of(context);
    return Semantics(
      container: true,
      explicitChildNodes: true,
      label: 'Application taskbar',
      child: Container(
        height: retcon.largeTargets
            ? RetconDimensions.accessibleTaskbarHeight
            : RetconDimensions.taskbarHeight,
        decoration: BoxDecoration(
          color: retcon.highContrast ? Colors.black : null,
          gradient: retcon.highContrast
              ? null
              : const LinearGradient(
                  colors: [
                    RetconColors.titleBarTop,
                    RetconColors.titleBarBottom,
                  ],
                ),
          border: Border(top: BorderSide(color: retcon.borderColor)),
        ),
        child: child,
      ),
    );
  }
}

/// Start button with selected/expanded semantics and a keyboard-sized target.
class RetconStartButton extends StatelessWidget {
  const RetconStartButton({
    required this.onPressed,
    super.key,
    this.expanded = false,
    this.label = 'start',
  });

  final VoidCallback? onPressed;
  final bool expanded;
  final String label;

  @override
  Widget build(BuildContext context) {
    final retcon = RetconTheme.of(context);
    return Semantics(
      button: true,
      enabled: onPressed != null,
      expanded: expanded,
      label: '$label menu',
      child: ConstrainedBox(
        constraints: BoxConstraints(
          minHeight: retcon.minimumTargetSize,
          minWidth: 88,
        ),
        child: TextButton.icon(
          onPressed: onPressed,
          style: TextButton.styleFrom(
            foregroundColor: Theme.of(context).colorScheme.onSurface,
            backgroundColor: expanded
                ? RetconColors.startGreen.withValues(alpha: 0.72)
                : RetconColors.startGreen,
            shape: const RoundedRectangleBorder(
              borderRadius: BorderRadius.only(
                topRight: Radius.circular(10),
                bottomRight: Radius.circular(10),
              ),
            ),
          ),
          icon: const Icon(Icons.window, size: RetconIconSizes.standard),
          label: Text(label),
        ),
      ),
    );
  }
}

/// Raised Start-menu surface. Its children remain normal focusable controls.
class RetconStartMenu extends StatelessWidget {
  const RetconStartMenu({required this.children, super.key, this.width = 280});

  final List<Widget> children;
  final double width;

  @override
  Widget build(BuildContext context) => Semantics(
    container: true,
    label: 'Start menu',
    child: Material(
      elevation: 12,
      child: SizedBox(
        width: width,
        child: RetconPanel(
          label: 'Start menu',
          padding: const EdgeInsets.all(RetconSpacing.xs),
          child: FocusTraversalGroup(
            policy: OrderedTraversalPolicy(),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                if (children.isNotEmpty)
                  Focus(autofocus: true, child: children.first),
                ...children.skip(1),
              ],
            ),
          ),
        ),
      ),
    ),
  );
}

/// Bordered notification area for counters, provider state, and the clock.
class RetconNotificationArea extends StatelessWidget {
  const RetconNotificationArea({required this.children, super.key});

  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    final retcon = RetconTheme.of(context);
    return Semantics(
      container: true,
      explicitChildNodes: true,
      label: 'Notification area',
      child: Container(
        constraints: BoxConstraints(minHeight: retcon.minimumTargetSize),
        padding: const EdgeInsets.symmetric(horizontal: RetconSpacing.sm),
        decoration: BoxDecoration(
          color: retcon.highContrast
              ? Colors.black
              : RetconColors.surface.withValues(alpha: 0.55),
          border: Border(left: BorderSide(color: retcon.borderColor)),
        ),
        child: Row(mainAxisSize: MainAxisSize.min, children: children),
      ),
    );
  }
}

/// Clock that updates once per minute and exposes the full local time to assistive tech.
class RetconClock extends StatefulWidget {
  const RetconClock({super.key, this.now});

  final DateTime Function()? now;

  @override
  State<RetconClock> createState() => _RetconClockState();
}

class _RetconClockState extends State<RetconClock> {
  Timer? _timer;

  DateTime get _now => (widget.now ?? DateTime.now)();

  @override
  void initState() {
    super.initState();
    _timer = Timer.periodic(const Duration(minutes: 1), (_) {
      if (mounted) setState(() {});
    });
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final value = _now;
    final hour = value.hour == 0
        ? 12
        : (value.hour > 12 ? value.hour - 12 : value.hour);
    final minute = value.minute.toString().padLeft(2, '0');
    final period = value.hour >= 12 ? 'PM' : 'AM';
    final label = '$hour:$minute $period';
    return Semantics(
      label: 'Local time $label',
      excludeSemantics: true,
      child: Text(label),
    );
  }
}

enum RetconIndicatorState { inactive, active, warning, error }

/// Color-independent status light with icon and text semantics.
class RetconStatusIndicator extends StatelessWidget {
  const RetconStatusIndicator({
    required this.label,
    required this.state,
    super.key,
    this.showLabel = true,
  });

  final String label;
  final RetconIndicatorState state;
  final bool showLabel;

  @override
  Widget build(BuildContext context) {
    final retcon = RetconTheme.of(context);
    final (color, icon) = switch (state) {
      RetconIndicatorState.inactive => (
        retcon.mutedTextColor,
        Icons.pause_circle_outline,
      ),
      RetconIndicatorState.active => (
        retcon.successColor,
        Icons.play_circle_fill,
      ),
      RetconIndicatorState.warning => (
        retcon.warningColor,
        Icons.warning_amber,
      ),
      RetconIndicatorState.error => (
        Theme.of(context).colorScheme.error,
        Icons.error,
      ),
    };
    return Semantics(
      label: '${state.name}: $label',
      excludeSemantics: true,
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(icon, size: RetconIconSizes.standard, color: color),
          if (showLabel) ...[
            const SizedBox(width: RetconSpacing.xs),
            Text(label),
          ],
        ],
      ),
    );
  }
}

/// Desktop shortcut with keyboard activation and selection semantics.
class RetconDesktopIcon extends StatelessWidget {
  const RetconDesktopIcon({
    required this.label,
    required this.icon,
    required this.onPressed,
    super.key,
    this.selected = false,
  });

  final String label;
  final IconData icon;
  final VoidCallback? onPressed;
  final bool selected;

  @override
  Widget build(BuildContext context) => Semantics(
    button: true,
    selected: selected,
    enabled: onPressed != null,
    label: label,
    child: InkWell(
      onTap: onPressed,
      child: Container(
        width: 88,
        padding: const EdgeInsets.all(RetconSpacing.sm),
        color: selected ? RetconColors.selection : null,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, size: RetconIconSizes.desktop),
            const SizedBox(height: RetconSpacing.xs),
            Text(label, textAlign: TextAlign.center),
          ],
        ),
      ),
    ),
  );
}

/// Persistent balloon notification for actionable desktop events.
class RetconBalloonNotification extends StatelessWidget {
  const RetconBalloonNotification({
    required this.title,
    required this.message,
    super.key,
    this.status = RetconStatus.neutral,
    this.onDismiss,
  });

  final String title;
  final String message;
  final RetconStatus status;
  final VoidCallback? onDismiss;

  @override
  Widget build(BuildContext context) => Semantics(
    liveRegion: true,
    container: true,
    label: '$title. $message',
    child: ConstrainedBox(
      constraints: const BoxConstraints(maxWidth: 320),
      child: RetconPanel(
        label: title,
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            RetconBadge(label: title, status: status),
            const SizedBox(width: RetconSpacing.sm),
            Expanded(child: Text(message)),
            if (onDismiss != null)
              SizedBox.square(
                dimension: RetconTheme.of(context).minimumTargetSize,
                child: IconButton(
                  tooltip: 'Dismiss $title',
                  constraints: BoxConstraints.tightFor(
                    width: RetconTheme.of(context).minimumTargetSize,
                    height: RetconTheme.of(context).minimumTargetSize,
                  ),
                  onPressed: onDismiss,
                  icon: const Icon(Icons.close),
                ),
              ),
          ],
        ),
      ),
    ),
  );
}

/// Semantic aliases used where the UI needs domain-specific status names.
typedef RetconActivityLight = RetconStatusIndicator;
typedef RetconConnectionIndicator = RetconStatusIndicator;
