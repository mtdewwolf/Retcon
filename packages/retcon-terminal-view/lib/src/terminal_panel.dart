import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

import 'ansi_parser.dart';
import 'terminal_controller.dart';
import 'terminal_service.dart';

/// Workspace terminal panel with tabs, ANSI rendering, resize, and clipboard.
class TerminalPanel extends StatefulWidget {
  const TerminalPanel({
    super.key,
    required this.service,
    this.events,
    this.cwd,
  });

  final TerminalService service;
  final Stream<Map<String, dynamic>>? events;
  final String? cwd;

  @override
  State<TerminalPanel> createState() => _TerminalPanelState();
}

class _TerminalPanelState extends State<TerminalPanel> {
  late final TerminalController _controller = TerminalController(
    service: widget.service,
    events: widget.events,
  );
  final _scrollController = ScrollController();
  final _focusNode = FocusNode();
  Size? _lastSize;
  static const _cellWidth = 7.8;
  static const _cellHeight = 16.0;

  @override
  void initState() {
    super.initState();
    _controller.addListener(_handleControllerUpdate);
    unawaited(_controller.initialize(cwd: widget.cwd));
  }

  void _handleControllerUpdate() {
    if (!mounted) return;
    setState(() {});
    if (_scrollController.hasClients) {
      unawaited(
        _scrollController.animateTo(
          _scrollController.position.maxScrollExtent,
          duration: const Duration(milliseconds: 80),
          curve: Curves.easeOut,
        ),
      );
    }
  }

  @override
  void dispose() {
    _controller.removeListener(_handleControllerUpdate);
    _controller.dispose();
    _scrollController.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  Future<void> _handleResize(Size size) async {
    if (_lastSize == size) return;
    _lastSize = size;
    final cols = (size.width / _cellWidth).floor();
    final rows = (size.height / _cellHeight).floor();
    await _controller.resize(cols, rows);
  }

  KeyEventResult _handleKey(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent) return KeyEventResult.ignored;
    final tab = _controller.activeTab;
    if (tab == null || !tab.alive) return KeyEventResult.ignored;

    if (event.logicalKey == LogicalKeyboardKey.keyC &&
        HardwareKeyboard.instance.isControlPressed) {
      final selection = _selectedText();
      if (selection != null && selection.isNotEmpty) {
        unawaited(Clipboard.setData(ClipboardData(text: selection)));
        return KeyEventResult.handled;
      }
    }
    if (event.logicalKey == LogicalKeyboardKey.keyV &&
        HardwareKeyboard.instance.isControlPressed) {
      unawaited(_paste());
      return KeyEventResult.handled;
    }

    final data = _encodeKey(event);
    if (data == null) return KeyEventResult.ignored;
    unawaited(_controller.sendInput(data));
    return KeyEventResult.handled;
  }

  Future<void> _paste() async {
    final data = await Clipboard.getData('text/plain');
    final text = data?.text;
    if (text == null || text.isEmpty) return;
    await _controller.sendInput(text);
  }

  String? _encodeKey(KeyDownEvent event) {
    if (event.character != null &&
        event.character!.isNotEmpty &&
        !HardwareKeyboard.instance.isControlPressed &&
        !HardwareKeyboard.instance.isAltPressed) {
      return event.character;
    }
    return switch (event.logicalKey) {
      LogicalKeyboardKey.enter => '\r',
      LogicalKeyboardKey.backspace => '\x7f',
      LogicalKeyboardKey.tab => '\t',
      LogicalKeyboardKey.arrowUp => '\u001b[A',
      LogicalKeyboardKey.arrowDown => '\u001b[B',
      LogicalKeyboardKey.arrowRight => '\u001b[C',
      LogicalKeyboardKey.arrowLeft => '\u001b[D',
      LogicalKeyboardKey.delete => '\u001b[3~',
      LogicalKeyboardKey.escape => '\u001b',
      _ => null,
    };
  }

  String? _selectedText() {
    // SelectionArea handles copy via context menu; Ctrl+C uses stripped output tail.
    final tab = _controller.activeTab;
    if (tab == null) return null;
    return AnsiParser.strip(tab.output);
  }

  @override
  Widget build(BuildContext context) {
    if (_controller.loading) {
      return const Center(child: CircularProgressIndicator());
    }
    if (_controller.error != null) {
      return Center(child: Text(_controller.error!));
    }
    return Column(
      children: [
        _TerminalTabBar(controller: _controller),
        Expanded(
          child: LayoutBuilder(
            builder: (context, constraints) {
              final size = Size(constraints.maxWidth, constraints.maxHeight);
              unawaited(_handleResize(size));
              return Focus(
                autofocus: true,
                focusNode: _focusNode,
                onKeyEvent: _handleKey,
                child: GestureDetector(
                  onTap: () => _focusNode.requestFocus(),
                  child: Container(
                    color: const Color(0xFF1E1E1E),
                    padding: const EdgeInsets.all(RetconSpacing.sm),
                    child: SelectionArea(
                      contextMenuBuilder: (context, editableTextState) {
                        return AdaptiveTextSelectionToolbar.buttonItems(
                          anchors: editableTextState.contextMenuAnchors,
                          buttonItems: [
                            ContextMenuButtonItem(
                              onPressed: () {
                                ContextMenuController.removeAny();
                                final selection = editableTextState
                                    .textEditingValue
                                    .selection;
                                final text = editableTextState
                                    .textEditingValue
                                    .text;
                                if (selection.isValid) {
                                  unawaited(
                                    Clipboard.setData(
                                      ClipboardData(
                                        text: text.substring(
                                          selection.start,
                                          selection.end,
                                        ),
                                      ),
                                    ),
                                  );
                                }
                              },
                              label: 'Copy',
                            ),
                            ContextMenuButtonItem(
                              onPressed: () {
                                ContextMenuController.removeAny();
                                unawaited(_paste());
                              },
                              label: 'Paste',
                            ),
                          ],
                        );
                      },
                      child: SingleChildScrollView(
                        controller: _scrollController,
                        child: RichText(
                          text: TextSpan(
                            children: AnsiParser.parse(
                              _controller.activeTab?.output ?? '',
                              baseStyle: const TextStyle(
                                fontFamily: 'Consolas',
                                fontFamilyFallback: [
                                  'Courier New',
                                  'monospace',
                                ],
                                fontSize: 13,
                                height: 1.2,
                                color: Color(0xFFCCCCCC),
                              ),
                            ),
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
              );
            },
          ),
        ),
      ],
    );
  }
}

class _TerminalTabBar extends StatelessWidget {
  const _TerminalTabBar({required this.controller});
  final TerminalController controller;

  @override
  Widget build(BuildContext context) {
    return Container(
      height: 34,
      color: RetconColors.titleBarInactive,
      child: Row(
        children: [
          Expanded(
            child: SingleChildScrollView(
              scrollDirection: Axis.horizontal,
              child: Row(
                children: [
                  for (var index = 0; index < controller.tabs.length; index++)
                    _TerminalTabChip(
                      tab: controller.tabs[index],
                      selected: index == controller.activeIndex,
                      onSelect: () => controller.selectTab(index),
                      onClose: () => unawaited(controller.closeTab(index)),
                    ),
                ],
              ),
            ),
          ),
          IconButton(
            tooltip: 'New terminal',
            icon: const Icon(Icons.add, size: 18),
            onPressed: () => unawaited(controller.createTab()),
          ),
        ],
      ),
    );
  }
}

class _TerminalTabChip extends StatelessWidget {
  const _TerminalTabChip({
    required this.tab,
    required this.selected,
    required this.onSelect,
    required this.onClose,
  });
  final TerminalTab tab;
  final bool selected;
  final VoidCallback onSelect;
  final VoidCallback onClose;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(left: 2),
      child: InputChip(
        label: Text(tab.title),
        avatar: Icon(
          tab.alive ? Icons.circle : Icons.circle_outlined,
          size: 10,
          color: tab.alive ? Colors.greenAccent : RetconColors.textMuted,
        ),
        selected: selected,
        onPressed: onSelect,
        deleteIcon: const Icon(Icons.close, size: 16),
        onDeleted: onClose,
      ),
    );
  }
}
