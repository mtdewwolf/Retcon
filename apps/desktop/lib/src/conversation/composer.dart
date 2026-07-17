import 'package:flutter/material.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

import 'conversation_models.dart';

/// Multiline prompt input with send/stop and provider selection.
class ConversationComposer extends StatefulWidget {
  const ConversationComposer({
    required this.providers,
    required this.selectedProviderId,
    required this.selectedModel,
    required this.isTurnActive,
    required this.enabled,
    required this.onSend,
    required this.onStop,
    required this.onProviderChanged,
    required this.onModelChanged,
    super.key,
  });

  final List<ProviderOption> providers;
  final String selectedProviderId;
  final String selectedModel;
  final bool isTurnActive;
  final bool enabled;
  final ValueChanged<String> onSend;
  final VoidCallback onStop;
  final ValueChanged<String> onProviderChanged;
  final ValueChanged<String> onModelChanged;

  @override
  State<ConversationComposer> createState() => _ConversationComposerState();
}

class _ConversationComposerState extends State<ConversationComposer> {
  final _controller = TextEditingController();
  final _focusNode = FocusNode();

  @override
  void dispose() {
    _controller.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  void _submit() {
    final text = _controller.text.trim();
    if (text.isEmpty || widget.isTurnActive || !widget.enabled) return;
    widget.onSend(text);
    _controller.clear();
  }

  @override
  Widget build(BuildContext context) {
    final selectedProvider = widget.providers
        .where((provider) => provider.id == widget.selectedProviderId)
        .firstOrNull;
    final models = selectedProvider?.models ?? const ['default'];

    return Padding(
      padding: const EdgeInsets.all(RetconSpacing.md),
      child: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          mainAxisSize: MainAxisSize.min,
          children: [
            Wrap(
              spacing: RetconSpacing.sm,
              runSpacing: RetconSpacing.sm,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: [
                SizedBox(
                  width: 220,
                  child: RetconDropdown<String>(
                    label: 'Provider',
                    value: widget.selectedProviderId,
                    enabled: widget.enabled && !widget.isTurnActive,
                    items: [
                      for (final provider in widget.providers)
                        RetconDropdownItem(
                          value: provider.id,
                          label: provider.label,
                        ),
                    ],
                    onChanged: (value) {
                      if (value != null) widget.onProviderChanged(value);
                    },
                  ),
                ),
                if (models.length > 1)
                  SizedBox(
                    width: 180,
                    child: RetconDropdown<String>(
                      label: 'Model',
                      value: widget.selectedModel,
                      enabled: widget.enabled && !widget.isTurnActive,
                      items: [
                        for (final model in models)
                          RetconDropdownItem(value: model, label: model),
                      ],
                      onChanged: (value) {
                        if (value != null) widget.onModelChanged(value);
                      },
                    ),
                  ),
              ],
            ),
            const SizedBox(height: RetconSpacing.sm),
            TextField(
              controller: _controller,
              focusNode: _focusNode,
              enabled: widget.enabled && !widget.isTurnActive,
              minLines: 2,
              maxLines: 6,
              textInputAction: TextInputAction.newline,
              decoration: const InputDecoration(
                labelText: 'Message',
                hintText: 'Describe what you want the agent to do…',
                alignLabelWithHint: true,
              ),
              onSubmitted: widget.isTurnActive ? null : (_) => _submit(),
            ),
            const SizedBox(height: RetconSpacing.sm),
            Row(
              children: [
                if (widget.isTurnActive)
                  RetconButton(
                    label: 'Stop',
                    icon: Icons.stop_circle_outlined,
                    onPressed: widget.enabled ? widget.onStop : null,
                  )
                else
                  RetconButton(
                    label: 'Send',
                    icon: Icons.send,
                    onPressed: widget.enabled ? _submit : null,
                  ),
                const Spacer(),
                if (!widget.enabled)
                  Expanded(
                    child: Text(
                      'Connect to Retcon Core to chat with an agent.',
                      textAlign: TextAlign.end,
                      style: Theme.of(context).textTheme.labelSmall,
                    ),
                  ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
