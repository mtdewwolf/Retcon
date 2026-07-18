import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../theme.dart';
import '../tokens.dart';
import 'surfaces.dart';

/// Menu item used by [RetconMenu] and [RetconContextMenu].
class RetconMenuItem {
  const RetconMenuItem({
    required this.label,
    this.icon,
    this.onSelected,
    this.enabled = true,
    this.shortcut,
  });

  final String label;
  final IconData? icon;
  final VoidCallback? onSelected;
  final bool enabled;
  final LogicalKeySet? shortcut;
}

/// Luna Dark tooltip that respects reduced-motion wait durations.
class RetconTooltip extends StatelessWidget {
  const RetconTooltip({
    required this.message,
    required this.child,
    super.key,
    this.semanticsLabel,
  });

  final String message;
  final Widget child;
  final String? semanticsLabel;

  @override
  Widget build(BuildContext context) {
    final retcon = RetconTheme.of(context);
    return Semantics(
      label: semanticsLabel ?? message,
      tooltip: message,
      child: Tooltip(
        message: message,
        waitDuration: retcon.reducedMotion
            ? Duration.zero
            : const Duration(milliseconds: 500),
        showDuration: retcon.reducedMotion
            ? const Duration(seconds: 8)
            : const Duration(seconds: 2),
        child: child,
      ),
    );
  }
}

/// Modal dialog with Luna Dark chrome and keyboard-dismiss support.
class RetconDialog extends StatelessWidget {
  const RetconDialog({
    required this.title,
    required this.content,
    super.key,
    this.actions = const [],
    this.semanticsLabel,
  });

  final String title;
  final Widget content;
  final List<Widget> actions;
  final String? semanticsLabel;

  @override
  Widget build(BuildContext context) {
    final retcon = RetconTheme.of(context);
    return Semantics(
      namesRoute: true,
      label: semanticsLabel ?? title,
      child: Dialog(
        backgroundColor: Theme.of(context).colorScheme.surface,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(RetconBorders.dialogRadius),
          side: BorderSide(color: retcon.borderColor),
        ),
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 480),
          child: RetconPanel(
            label: title,
            padding: const EdgeInsets.all(RetconSpacing.xl),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Semantics(
                  header: true,
                  child: Text(
                    title,
                    style: Theme.of(context).textTheme.titleMedium,
                  ),
                ),
                const SizedBox(height: RetconSpacing.md),
                Flexible(child: SingleChildScrollView(child: content)),
                if (actions.isNotEmpty) ...[
                  const SizedBox(height: RetconSpacing.lg),
                  Wrap(
                    alignment: WrapAlignment.end,
                    spacing: RetconSpacing.sm,
                    runSpacing: RetconSpacing.sm,
                    children: actions,
                  ),
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// Shows a [RetconDialog] and returns when it closes.
Future<T?> showRetconDialog<T>({
  required BuildContext context,
  required String title,
  required Widget content,
  List<Widget> actions = const [],
  String? semanticsLabel,
  bool barrierDismissible = true,
}) => showDialog<T>(
  context: context,
  barrierDismissible: barrierDismissible,
  builder: (context) => RetconDialog(
    title: title,
    content: content,
    actions: actions,
    semanticsLabel: semanticsLabel,
  ),
);

/// Dropdown option for [RetconDropdown].
class RetconDropdownItem<T> {
  const RetconDropdownItem({required this.value, required this.label});

  final T value;
  final String label;
}

/// Labeled dropdown with stable semantics and keyboard focus ring.
class RetconDropdown<T> extends StatelessWidget {
  const RetconDropdown({
    required this.label,
    required this.items,
    super.key,
    this.value,
    this.onChanged,
    this.hint,
    this.enabled = true,
  });

  final String label;
  final List<RetconDropdownItem<T>> items;
  final T? value;
  final ValueChanged<T?>? onChanged;
  final String? hint;
  final bool enabled;

  @override
  Widget build(BuildContext context) => Semantics(
    label: label,
    enabled: enabled,
    value: _labelForValue(value),
    child: InputDecorator(
      decoration: InputDecoration(
        labelText: label,
        hintText: hint,
        enabled: enabled,
      ),
      isEmpty: value == null,
      child: DropdownButtonHideUnderline(
        child: DropdownButton<T>(
          value: value,
          isExpanded: true,
          hint: hint == null ? null : Text(hint!),
          items: [
            for (final item in items)
              DropdownMenuItem<T>(value: item.value, child: Text(item.label)),
          ],
          onChanged: enabled ? onChanged : null,
        ),
      ),
    ),
  );

  String? _labelForValue(T? selected) {
    if (selected == null) return null;
    for (final item in items) {
      if (item.value == selected) return item.label;
    }
    return selected.toString();
  }
}

/// Menu button that opens a popup list of [RetconMenuItem]s.
class RetconMenu extends StatelessWidget {
  const RetconMenu({
    required this.label,
    required this.items,
    super.key,
    this.icon,
  });

  final String label;
  final List<RetconMenuItem> items;
  final IconData? icon;

  @override
  Widget build(BuildContext context) {
    final retcon = RetconTheme.of(context);
    return Semantics(
      button: true,
      label: label,
      child: PopupMenuButton<RetconMenuItem>(
        tooltip: label,
        itemBuilder: (context) => [
          for (final item in items)
            PopupMenuItem<RetconMenuItem>(
              value: item,
              enabled: item.enabled,
              child: _MenuItemRow(item: item),
            ),
        ],
        onSelected: (item) => item.onSelected?.call(),
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(RetconBorders.radius),
          side: BorderSide(color: retcon.borderColor),
        ),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: RetconSpacing.sm),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (icon != null) ...[
                Icon(icon, size: RetconIconSizes.standard),
                const SizedBox(width: RetconSpacing.xs),
              ],
              Text(label),
              const SizedBox(width: RetconSpacing.xxs),
              const Icon(Icons.arrow_drop_down, size: RetconIconSizes.standard),
            ],
          ),
        ),
      ),
    );
  }
}

class _MenuItemRow extends StatelessWidget {
  const _MenuItemRow({required this.item});

  final RetconMenuItem item;

  @override
  Widget build(BuildContext context) => Row(
    children: [
      if (item.icon != null) ...[
        Icon(item.icon, size: RetconIconSizes.standard),
        const SizedBox(width: RetconSpacing.sm),
      ],
      Expanded(child: Text(item.label)),
      if (item.shortcut != null) ...[
        const SizedBox(width: RetconSpacing.md),
        Text(
          item.shortcut!.toString().split('.').last,
          style: Theme.of(context).textTheme.labelSmall,
        ),
      ],
    ],
  );
}

/// Right-click or Shift+F10 context menu wrapper.
class RetconContextMenu extends StatelessWidget {
  const RetconContextMenu({
    required this.child,
    required this.items,
    super.key,
    this.semanticsLabel = 'Context menu region',
  });

  final Widget child;
  final List<RetconMenuItem> items;
  final String semanticsLabel;

  Future<void> _showMenu(BuildContext context, Offset position) async {
    final retcon = RetconTheme.of(context);
    final selected = await showMenu<RetconMenuItem>(
      context: context,
      position: RelativeRect.fromLTRB(
        position.dx,
        position.dy,
        position.dx,
        position.dy,
      ),
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(RetconBorders.radius),
        side: BorderSide(color: retcon.borderColor),
      ),
      items: [
        for (final item in items)
          PopupMenuItem<RetconMenuItem>(
            value: item,
            enabled: item.enabled,
            child: _MenuItemRow(item: item),
          ),
      ],
    );
    selected?.onSelected?.call();
  }

  @override
  Widget build(BuildContext context) => Semantics(
    container: true,
    label: semanticsLabel,
    hint: 'Right-click or Shift+F10 for menu',
    onLongPress: () {},
    child: GestureDetector(
      onSecondaryTapDown: (details) =>
          _showMenu(context, details.globalPosition),
      child: Focus(
        onKeyEvent: (node, event) {
          if (event is KeyDownEvent &&
              event.logicalKey == LogicalKeyboardKey.f10 &&
              HardwareKeyboard.instance.isShiftPressed) {
            final box = context.findRenderObject() as RenderBox?;
            final offset = box?.localToGlobal(Offset.zero) ?? Offset.zero;
            _showMenu(context, offset);
            return KeyEventResult.handled;
          }
          return KeyEventResult.ignored;
        },
        child: child,
      ),
    ),
  );
}

enum RetconNotificationLevel { info, success, warning, error }

/// Transient notification surfaced as a themed snack bar.
class RetconNotification {
  RetconNotification._();

  static void show(
    BuildContext context, {
    required String message,
    RetconNotificationLevel level = RetconNotificationLevel.info,
    Duration? duration,
    String? actionLabel,
    VoidCallback? onAction,
  }) {
    final retcon = RetconTheme.of(context);
    final (icon, color) = switch (level) {
      RetconNotificationLevel.success => (
        Icons.check_circle,
        retcon.successColor,
      ),
      RetconNotificationLevel.warning => (
        Icons.warning_amber,
        retcon.warningColor,
      ),
      RetconNotificationLevel.error => (
        Icons.error,
        Theme.of(context).colorScheme.error,
      ),
      RetconNotificationLevel.info => (
        Icons.info_outline,
        retcon.mutedTextColor,
      ),
    };

    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(
        duration:
            duration ??
            (retcon.reducedMotion
                ? const Duration(seconds: 8)
                : const Duration(seconds: 4)),
        behavior: SnackBarBehavior.floating,
        backgroundColor: retcon.highContrast
            ? Colors.black
            : Theme.of(context).colorScheme.surface,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(RetconBorders.radius),
          side: BorderSide(color: color),
        ),
        content: Semantics(
          liveRegion: true,
          label: '${level.name}: $message',
          child: Row(
            children: [
              Icon(icon, color: color, size: RetconIconSizes.standard),
              const SizedBox(width: RetconSpacing.sm),
              Expanded(child: Text(message)),
            ],
          ),
        ),
        action: actionLabel == null
            ? null
            : SnackBarAction(label: actionLabel, onPressed: onAction ?? () {}),
      ),
    );
  }
}
