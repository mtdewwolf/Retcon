import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../theme.dart';
import '../tokens.dart';

/// Resizable split pane with keyboard-adjustable divider and semantics.
class RetconSplitter extends StatefulWidget {
  const RetconSplitter({
    required this.first,
    required this.second,
    super.key,
    this.axis = Axis.horizontal,
    this.initialRatio = 0.5,
    this.minFirst = RetconDimensions.minimumPanelWidth,
    this.minSecond = RetconDimensions.minimumPanelWidth,
    this.semanticsLabel = 'Split pane',
  });

  final Widget first;
  final Widget second;
  final Axis axis;
  final double initialRatio;
  final double minFirst;
  final double minSecond;
  final String semanticsLabel;

  @override
  State<RetconSplitter> createState() => _RetconSplitterState();
}

class _RetconSplitterState extends State<RetconSplitter> {
  late double _ratio;
  bool _dividerFocused = false;

  @override
  void initState() {
    super.initState();
    _ratio = widget.initialRatio.clamp(0.05, 0.95);
  }

  void _adjust(double delta, double total) {
    if (total <= 0) return;
    setState(() {
      final firstSize = _ratio * total + delta;
      final clampedFirst = firstSize.clamp(widget.minFirst, total - widget.minSecond);
      _ratio = (clampedFirst / total).clamp(0.05, 0.95);
    });
  }

  @override
  Widget build(BuildContext context) {
    final retcon = RetconTheme.of(context);
    final isHorizontal = widget.axis == Axis.horizontal;
    final dividerThickness = retcon.largeTargets ? RetconSpacing.md : RetconSpacing.sm;

    return LayoutBuilder(
      builder: (context, constraints) {
        final total = isHorizontal ? constraints.maxWidth : constraints.maxHeight;
        final firstSize = total * _ratio;
        final secondSize = total - firstSize - dividerThickness;

        return Semantics(
          label: widget.semanticsLabel,
          value: '${(_ratio * 100).round()}% first pane',
          child: Flex(
            direction: isHorizontal ? Axis.horizontal : Axis.vertical,
            children: [
              SizedBox(
                width: isHorizontal ? firstSize : null,
                height: isHorizontal ? null : firstSize,
                child: widget.first,
              ),
              Focus(
                onFocusChange: (focused) => setState(() => _dividerFocused = focused),
                onKeyEvent: (node, event) {
                  if (event is! KeyDownEvent) return KeyEventResult.ignored;
                  final step = retcon.largeTargets ? 24.0 : 16.0;
                  if (isHorizontal) {
                    if (event.logicalKey == LogicalKeyboardKey.arrowLeft) {
                      _adjust(-step, total);
                      return KeyEventResult.handled;
                    }
                    if (event.logicalKey == LogicalKeyboardKey.arrowRight) {
                      _adjust(step, total);
                      return KeyEventResult.handled;
                    }
                  } else {
                    if (event.logicalKey == LogicalKeyboardKey.arrowUp) {
                      _adjust(-step, total);
                      return KeyEventResult.handled;
                    }
                    if (event.logicalKey == LogicalKeyboardKey.arrowDown) {
                      _adjust(step, total);
                      return KeyEventResult.handled;
                    }
                  }
                  return KeyEventResult.ignored;
                },
                child: MouseRegion(
                  cursor: isHorizontal
                      ? SystemMouseCursors.resizeColumn
                      : SystemMouseCursors.resizeRow,
                  child: GestureDetector(
                    behavior: HitTestBehavior.translucent,
                    onPanUpdate: (details) {
                      final delta = isHorizontal ? details.delta.dx : details.delta.dy;
                      _adjust(delta, total);
                    },
                    child: AnimatedContainer(
                      duration: retcon.motion(RetconMotion.fast),
                      width: isHorizontal ? dividerThickness : double.infinity,
                      height: isHorizontal ? double.infinity : dividerThickness,
                      decoration: BoxDecoration(
                        color: retcon.highContrast
                            ? retcon.borderColor
                            : RetconColors.border,
                        border: _dividerFocused
                            ? Border.all(
                                color: retcon.focusColor,
                                width: RetconBorders.focus,
                              )
                            : null,
                      ),
                      child: Semantics(
                        slider: true,
                        label: 'Resize ${widget.semanticsLabel}',
                        value: '${(_ratio * 100).round()}%',
                        child: const SizedBox.expand(),
                      ),
                    ),
                  ),
                ),
              ),
              SizedBox(
                width: isHorizontal ? secondSize : null,
                height: isHorizontal ? null : secondSize,
                child: widget.second,
              ),
            ],
          ),
        );
      },
    );
  }
}
