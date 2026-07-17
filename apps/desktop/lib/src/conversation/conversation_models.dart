/// Typed conversation state for the agent panel.
enum ConversationMessageRole { user, assistant, system, error, tool }

enum ConversationSessionState {
  idle,
  preparing,
  running,
  waitingApproval,
  paused,
  completed,
  failed,
  cancelled,
}

/// One rendered row in the message list.
class ConversationMessage {
  const ConversationMessage({
    required this.id,
    required this.role,
    this.text = '',
    this.streaming = false,
    this.toolName,
    this.toolInput,
    this.toolOutput,
    this.errorMessage,
    this.timestamp,
  });

  final String id;
  final ConversationMessageRole role;
  final String text;
  final bool streaming;
  final String? toolName;
  final String? toolInput;
  final String? toolOutput;
  final String? errorMessage;
  final DateTime? timestamp;

  ConversationMessage copyWith({
    String? text,
    bool? streaming,
    String? toolName,
    String? toolInput,
    String? toolOutput,
    String? errorMessage,
  }) => ConversationMessage(
    id: id,
    role: role,
    text: text ?? this.text,
    streaming: streaming ?? this.streaming,
    toolName: toolName ?? this.toolName,
    toolInput: toolInput ?? this.toolInput,
    toolOutput: toolOutput ?? this.toolOutput,
    errorMessage: errorMessage ?? this.errorMessage,
    timestamp: timestamp,
  );
}

/// Token and cost counters surfaced in the session header.
class TokenUsage {
  const TokenUsage({
    this.inputTokens = 0,
    this.outputTokens = 0,
    this.costUsd,
  });

  final int inputTokens;
  final int outputTokens;
  final double? costUsd;

  int get totalTokens => inputTokens + outputTokens;

  TokenUsage merge(TokenUsage other) => TokenUsage(
    inputTokens: other.inputTokens > 0 ? other.inputTokens : inputTokens,
    outputTokens: other.outputTokens > 0 ? other.outputTokens : outputTokens,
    costUsd: other.costUsd ?? costUsd,
  );
}

/// Approval prompt shown read-only until Wave 4 wiring lands.
class PendingApproval {
  const PendingApproval({
    required this.id,
    required this.title,
    this.detail,
  });

  final String id;
  final String title;
  final String? detail;
}

/// Provider option for the composer selector.
class ProviderOption {
  const ProviderOption({
    required this.id,
    required this.label,
    this.models = const [],
  });

  final String id;
  final String label;
  final List<String> models;
}
