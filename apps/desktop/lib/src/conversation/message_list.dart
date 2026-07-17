import 'package:flutter/material.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

import 'conversation_models.dart';

/// Scrollable transcript with streaming text, tool cards, and errors.
class MessageList extends StatefulWidget {
  const MessageList({
    required this.messages,
    super.key,
  });

  final List<ConversationMessage> messages;

  @override
  State<MessageList> createState() => _MessageListState();
}

class _MessageListState extends State<MessageList> {
  final _scrollController = ScrollController();

  @override
  void didUpdateWidget(covariant MessageList oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.messages.length != oldWidget.messages.length ||
        _lastText(widget.messages) != _lastText(oldWidget.messages)) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!_scrollController.hasClients) return;
        _scrollController.animateTo(
          _scrollController.position.maxScrollExtent,
          duration: const Duration(milliseconds: 120),
          curve: Curves.easeOut,
        );
      });
    }
  }

  String _lastText(List<ConversationMessage> messages) =>
      messages.isEmpty ? '' : messages.last.text;

  @override
  void dispose() {
    _scrollController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (widget.messages.isEmpty) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(RetconSpacing.lg),
          child: Text(
            'Send a message to start an agent session.',
            textAlign: TextAlign.center,
            style: Theme.of(context).textTheme.bodyLarge,
          ),
        ),
      );
    }

    return ListView.builder(
      controller: _scrollController,
      padding: const EdgeInsets.all(RetconSpacing.md),
      itemCount: widget.messages.length,
      itemBuilder: (context, index) =>
          _MessageBubble(message: widget.messages[index]),
    );
  }
}

class _MessageBubble extends StatelessWidget {
  const _MessageBubble({required this.message});

  final ConversationMessage message;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.only(bottom: RetconSpacing.sm),
      child: Align(
        alignment: _alignment(message.role),
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 720),
          child: switch (message.role) {
            ConversationMessageRole.tool => _ToolCard(message: message),
            ConversationMessageRole.error => RetconPanel(
              label: 'Error',
              padding: const EdgeInsets.all(RetconSpacing.sm),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Icon(
                    Icons.error_outline,
                    color: theme.colorScheme.error,
                    size: RetconIconSizes.standard,
                  ),
                  const SizedBox(width: RetconSpacing.sm),
                  Expanded(
                    child: Text(
                      message.errorMessage ?? message.text,
                      style: TextStyle(color: theme.colorScheme.error),
                    ),
                  ),
                ],
              ),
            ),
            _ => RetconPanel(
              label: _roleLabel(message.role),
              recessed: message.role == ConversationMessageRole.assistant,
              padding: const EdgeInsets.all(RetconSpacing.sm),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  SelectableText(
                    message.text.isEmpty && message.streaming
                        ? '…'
                        : message.text,
                    style: theme.textTheme.bodyMedium,
                  ),
                  if (message.streaming) ...[
                    const SizedBox(height: RetconSpacing.xs),
                    const RetconProgressBar(label: 'Streaming response'),
                  ],
                ],
              ),
            ),
          },
        ),
      ),
    );
  }

  static Alignment _alignment(ConversationMessageRole role) => switch (role) {
    ConversationMessageRole.user => Alignment.centerRight,
    ConversationMessageRole.error => Alignment.center,
    _ => Alignment.centerLeft,
  };

  static String _roleLabel(ConversationMessageRole role) => switch (role) {
    ConversationMessageRole.user => 'You',
    ConversationMessageRole.assistant => 'Agent',
    ConversationMessageRole.system => 'System',
    ConversationMessageRole.tool => 'Tool',
    ConversationMessageRole.error => 'Error',
  };
}

class _ToolCard extends StatelessWidget {
  const _ToolCard({required this.message});

  final ConversationMessage message;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return RetconPanel(
      label: message.toolName ?? 'Tool',
      padding: const EdgeInsets.all(RetconSpacing.sm),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(
                message.streaming ? Icons.build_circle : Icons.check_circle,
                size: RetconIconSizes.standard,
              ),
              const SizedBox(width: RetconSpacing.xs),
              Expanded(
                child: Text(
                  message.toolName ?? 'Tool',
                  style: theme.textTheme.titleSmall,
                ),
              ),
              if (message.streaming)
                const RetconBadge(label: 'Running', status: RetconStatus.warning),
            ],
          ),
          if (message.toolInput != null) ...[
            const SizedBox(height: RetconSpacing.xs),
            Text('Input', style: theme.textTheme.labelLarge),
            SelectableText(message.toolInput!),
          ],
          if (message.toolOutput != null) ...[
            const SizedBox(height: RetconSpacing.xs),
            Text('Output', style: theme.textTheme.labelLarge),
            SelectableText(message.toolOutput!),
          ],
        ],
      ),
    );
  }
}
