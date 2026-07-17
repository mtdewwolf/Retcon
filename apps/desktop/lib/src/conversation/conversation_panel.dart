import 'dart:async';

import 'package:flutter/material.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

import '../core_client.dart';
import 'composer.dart';
import 'conversation_controller.dart';
import 'message_list.dart';
import 'session_header.dart';

/// Agent conversation surface wired to [CoreClient.events].
class ConversationPanel extends StatefulWidget {
  const ConversationPanel({
    required this.core,
    super.key,
    this.workingDirectory,
  });

  final CoreClient core;
  final String? workingDirectory;

  @override
  State<ConversationPanel> createState() => _ConversationPanelState();
}

class _ConversationPanelState extends State<ConversationPanel> {
  late ConversationController _controller;

  @override
  void initState() {
    super.initState();
    _controller = ConversationController(
      core: widget.core,
      workingDirectory: widget.workingDirectory,
    );
  }

  @override
  void didUpdateWidget(covariant ConversationPanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.workingDirectory != widget.workingDirectory) {
      _controller.workingDirectory = widget.workingDirectory;
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final connected = widget.core.status == CoreConnectionStatus.connected;
    return AnimatedBuilder(
      animation: Listenable.merge([_controller, widget.core]),
      builder: (context, _) {
        final provider = _controller.providers
            .where((item) => item.id == _controller.selectedProviderId)
            .firstOrNull;
        return RetconSplitter(
          axis: Axis.vertical,
          initialRatio: 0.72,
          semanticsLabel: 'Conversation layout',
          first: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              SessionHeader(
                state: _controller.sessionState,
                usage: _controller.usage,
                pendingApprovals: _controller.pendingApprovals,
                error: _controller.sessionError,
                providerLabel: provider?.label,
                onApprove: (approval) =>
                    unawaited(_controller.decideApproval(approval, 'approve')),
                onDeny: (approval) =>
                    unawaited(_controller.decideApproval(approval, 'deny')),
              ),
              const SizedBox(height: RetconSpacing.xs),
              Expanded(child: MessageList(messages: _controller.messages)),
            ],
          ),
          second: ConversationComposer(
            providers: _controller.providers,
            selectedProviderId: _controller.selectedProviderId,
            selectedModel: _controller.selectedModel,
            isTurnActive: _controller.isTurnActive,
            enabled: connected,
            onSend: _controller.sendMessage,
            onStop: _controller.stopTurn,
            onProviderChanged: _controller.selectProvider,
            onModelChanged: _controller.selectModel,
          ),
        );
      },
    );
  }
}
