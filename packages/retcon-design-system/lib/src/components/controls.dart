import 'package:flutter/material.dart';

import '../theme.dart';
import '../tokens.dart';

/// Luna Dark push button with an explicit keyboard-focus indicator.
class RetconButton extends StatefulWidget {
  const RetconButton({
    required this.label,
    required this.onPressed,
    super.key,
    this.icon,
    this.autofocus = false,
  });

  final String label;
  final VoidCallback? onPressed;
  final IconData? icon;
  final bool autofocus;

  @override
  State<RetconButton> createState() => _RetconButtonState();
}

class _RetconButtonState extends State<RetconButton> {
  bool _focused = false;

  @override
  Widget build(BuildContext context) {
    final retcon = RetconTheme.of(context);
    return Focus(
      onFocusChange: (focused) => setState(() => _focused = focused),
      child: AnimatedContainer(
        duration: retcon.motion(RetconMotion.fast),
        decoration: BoxDecoration(
          borderRadius: BorderRadius.circular(RetconBorders.dialogRadius),
          border: _focused
              ? Border.all(color: retcon.focusColor, width: RetconBorders.focus)
              : null,
        ),
        padding: const EdgeInsets.all(RetconSpacing.xxs),
        child: ElevatedButton(
          autofocus: widget.autofocus,
          onPressed: widget.onPressed,
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (widget.icon != null) ...[
                Icon(widget.icon, size: RetconIconSizes.standard),
                const SizedBox(width: RetconSpacing.xs),
              ],
              Text(widget.label),
            ],
          ),
        ),
      ),
    );
  }
}

/// Compact icon action that always exposes an accessible label and tooltip.
class RetconIconButton extends StatelessWidget {
  const RetconIconButton({
    required this.label,
    required this.icon,
    required this.onPressed,
    super.key,
  });

  final String label;
  final IconData icon;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    final retcon = RetconTheme.of(context);
    return Semantics(
      button: true,
      enabled: onPressed != null,
      label: label,
      child: IconButton(
        tooltip: label,
        onPressed: onPressed,
        icon: Icon(icon, size: RetconIconSizes.standard),
        constraints: BoxConstraints.tightFor(
          width: retcon.minimumTargetSize,
          height: retcon.minimumTargetSize,
        ),
      ),
    );
  }
}

/// Themed text input with stable label, helper, and error semantics.
class RetconTextField extends StatelessWidget {
  const RetconTextField({
    required this.label,
    super.key,
    this.controller,
    this.focusNode,
    this.hint,
    this.helperText,
    this.errorText,
    this.onChanged,
    this.enabled = true,
    this.obscureText = false,
  });

  final String label;
  final TextEditingController? controller;
  final FocusNode? focusNode;
  final String? hint;
  final String? helperText;
  final String? errorText;
  final ValueChanged<String>? onChanged;
  final bool enabled;
  final bool obscureText;

  @override
  Widget build(BuildContext context) => TextField(
    controller: controller,
    focusNode: focusNode,
    enabled: enabled,
    obscureText: obscureText,
    onChanged: onChanged,
    decoration: InputDecoration(
      labelText: label,
      hintText: hint,
      helperText: helperText,
      errorText: errorText,
    ),
  );
}

/// Color-independent checkbox with a full-row pointer target.
class RetconCheckbox extends StatelessWidget {
  const RetconCheckbox({
    required this.label,
    required this.value,
    required this.onChanged,
    super.key,
  });

  final String label;
  final bool value;
  final ValueChanged<bool?>? onChanged;

  @override
  Widget build(BuildContext context) => CheckboxListTile(
    dense: !RetconTheme.of(context).largeTargets,
    contentPadding: EdgeInsets.zero,
    controlAffinity: ListTileControlAffinity.leading,
    title: Text(label),
    value: value,
    onChanged: onChanged,
  );
}

/// Keyboard-navigable group for [RetconRadioButton] options.
class RetconRadioGroup<T> extends StatelessWidget {
  const RetconRadioGroup({
    required this.groupValue,
    required this.onChanged,
    required this.children,
    super.key,
  });

  final T? groupValue;
  final ValueChanged<T?> onChanged;
  final List<Widget> children;

  @override
  Widget build(BuildContext context) => RadioGroup<T>(
    groupValue: groupValue,
    onChanged: onChanged,
    child: Column(children: children),
  );
}

/// Color-independent radio option with a full-row pointer target.
class RetconRadioButton<T> extends StatelessWidget {
  const RetconRadioButton({
    required this.label,
    required this.value,
    super.key,
    this.enabled = true,
  });

  final String label;
  final T value;
  final bool enabled;

  @override
  Widget build(BuildContext context) => RadioListTile<T>(
    dense: !RetconTheme.of(context).largeTargets,
    contentPadding: EdgeInsets.zero,
    title: Text(label),
    value: value,
    enabled: enabled,
  );
}
