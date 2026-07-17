import 'dart:async';

import 'package:flutter/foundation.dart';

import '../core_client.dart';
import 'conversation_models.dart';

/// Drives the agent conversation panel from core RPC and the event stream.
class ConversationController extends ChangeNotifier {
  ConversationController({
    required CoreClient core,
    String? workingDirectory,
  }) : _core = core,
       _workingDirectory = workingDirectory {
    _events = _core.events.listen(_onEvent);
    _core.addListener(_handleCoreStatus);
    unawaited(_loadProviders());
  }

  final CoreClient _core;
  String? _workingDirectory;
  StreamSubscription<Map<String, dynamic>>? _events;

  ConversationSessionState _sessionState = ConversationSessionState.idle;
  final List<ConversationMessage> _messages = [];
  final List<PendingApproval> _pendingApprovals = [];
  TokenUsage _usage = const TokenUsage();
  int? _activeTurnId;
  String? _sessionError;
  String? _nativeSessionId;
  List<ProviderOption> _providers = const [
    ProviderOption(
      id: 'claude-code',
      label: 'Claude Code',
      models: ['default'],
    ),
  ];
  String _selectedProviderId = 'claude-code';
  String _selectedModel = 'default';
  bool _loadingProviders = false;

  ConversationSessionState get sessionState => _sessionState;
  List<ConversationMessage> get messages => List.unmodifiable(_messages);
  List<PendingApproval> get pendingApprovals =>
      List.unmodifiable(_pendingApprovals);
  TokenUsage get usage => _usage;
  bool get isTurnActive =>
      _activeTurnId != null ||
      _sessionState == ConversationSessionState.running ||
      _sessionState == ConversationSessionState.preparing;
  String? get sessionError => _sessionError;
  List<ProviderOption> get providers => _providers;
  String get selectedProviderId => _selectedProviderId;
  String get selectedModel => _selectedModel;
  bool get loadingProviders => _loadingProviders;
  String? get workingDirectory => _workingDirectory;

  set workingDirectory(String? value) {
    if (_workingDirectory == value) return;
    _workingDirectory = value;
    notifyListeners();
  }

  void selectProvider(String providerId) {
    if (_selectedProviderId == providerId) return;
    _selectedProviderId = providerId;
    final provider = _providers.where((p) => p.id == providerId).firstOrNull;
    if (provider != null && provider.models.isNotEmpty) {
      _selectedModel = provider.models.first;
    }
    notifyListeners();
  }

  void selectModel(String model) {
    if (_selectedModel == model) return;
    _selectedModel = model;
    notifyListeners();
  }

  Future<void> _loadProviders() async {
    if (_core.status != CoreConnectionStatus.connected) return;
    _loadingProviders = true;
    notifyListeners();
    try {
      final metadata = await _core.request('provider.metadata');
      final providers = <ProviderOption>[];
      final list = metadata['providers'] as List<dynamic>? ?? const [];
      for (final item in list) {
        if (item is! Map) continue;
        final map = item.cast<String, dynamic>();
        final id = map['id']?.toString();
        if (id == null || id.isEmpty) continue;
        providers.add(
          ProviderOption(
            id: id,
            label: map['display_name']?.toString() ?? id,
            models: _readModels(map),
          ),
        );
      }
      if (providers.isEmpty) {
        final detected = await _core.request('agent.detect');
        providers.add(
          ProviderOption(
            id: detected['id']?.toString() ?? 'claude-code',
            label: 'Claude Code ${detected['version'] ?? ''}'.trim(),
            models: const ['default'],
          ),
        );
      }
      _providers = providers;
      if (!_providers.any((p) => p.id == _selectedProviderId)) {
        _selectedProviderId = _providers.first.id;
        _selectedModel = _providers.first.models.first;
      }
    } on Object {
      // Keep the default provider when core is unavailable.
    } finally {
      _loadingProviders = false;
      notifyListeners();
    }
  }

  List<String> _readModels(Map<String, dynamic> provider) {
    final models = provider['models'];
    if (models is List && models.isNotEmpty) {
      return models.map((m) => m.toString()).toList();
    }
    return const ['default'];
  }

  Future<void> sendMessage(String prompt) async {
    final trimmed = prompt.trim();
    if (trimmed.isEmpty || isTurnActive) return;
    if (_core.status != CoreConnectionStatus.connected) {
      _sessionState = ConversationSessionState.failed;
      _sessionError = 'Retcon Core is disconnected.';
      notifyListeners();
      return;
    }
    final cwd = _workingDirectory;
    if (cwd == null || cwd.isEmpty) {
      _sessionError = 'Open a project before starting an agent session.';
      notifyListeners();
      return;
    }

    _sessionError = null;
    _sessionState = ConversationSessionState.preparing;
    _messages.add(
      ConversationMessage(
        id: _nextId('user'),
        role: ConversationMessageRole.user,
        text: trimmed,
        timestamp: DateTime.now(),
      ),
    );
    notifyListeners();

    try {
      final result = await _sendTurnRpc(trimmed, cwd);
      final turnId = (result['turnId'] as num?)?.toInt();
      _activeTurnId = turnId;
      _sessionState = ConversationSessionState.running;
      _appendAssistantShell(streaming: true);
      notifyListeners();
    } catch (error) {
      _sessionState = ConversationSessionState.failed;
      _sessionError = error.toString();
      _messages.add(
        ConversationMessage(
          id: _nextId('error'),
          role: ConversationMessageRole.error,
          errorMessage: _sessionError,
          timestamp: DateTime.now(),
        ),
      );
      notifyListeners();
    }
  }

  Future<Map<String, dynamic>> _sendTurnRpc(String prompt, String cwd) async {
    try {
      return await _core.request(
        'turn.send',
        params: {
          'prompt': prompt,
          'providerId': _selectedProviderId,
          if (_selectedModel != 'default') 'model': _selectedModel,
          if (_nativeSessionId != null) 'sessionId': _nativeSessionId,
        },
      );
    } on CoreRpcException {
      return _core.request(
        'agent.start',
        params: {
          'cwd': cwd,
          'prompt': prompt,
          if (_nativeSessionId != null) 'sessionId': _nativeSessionId,
        },
      );
    }
  }

  Future<void> stopTurn() async {
    if (_activeTurnId == null) return;
    final turnId = _activeTurnId!;
    try {
      try {
        await _core.request('turn.cancel', params: {'turnId': turnId});
      } on CoreRpcException {
        await _core.request('agent.cancel', params: {'id': turnId});
      }
    } on Object catch (error) {
      _sessionError = error.toString();
    } finally {
      _finishTurn(cancelled: true);
    }
  }

  void _handleCoreStatus() {
    if (_core.status == CoreConnectionStatus.connected) {
      unawaited(_loadProviders());
    }
  }

  void _onEvent(Map<String, dynamic> event) {
    final kind = _eventKind(event);
    final payload = _eventPayload(event);

    switch (kind) {
      case 'agent.line':
        _handleAgentLine(payload);
      case 'agent.exit':
        _handleAgentExit(payload);
      case 'approval.requested':
        _handleApproval(payload);
      case 'approval.decided':
        _pendingApprovals.removeWhere(
          (item) => item.id == payload['id']?.toString(),
        );
        if (_pendingApprovals.isEmpty &&
            _sessionState == ConversationSessionState.waitingApproval) {
          _sessionState = ConversationSessionState.idle;
        }
        notifyListeners();
      case 'session.started':
      case 'session.resumed':
        _sessionState = ConversationSessionState.running;
        _nativeSessionId =
            payload['sessionId']?.toString() ??
            payload['native_session_id']?.toString();
        notifyListeners();
      case 'session.completed':
      case 'session.cancelled':
        _finishTurn(
          cancelled: kind == 'session.cancelled',
          failed: false,
        );
      case 'session.failed':
        _finishTurn(failed: true, error: payload['detail']?.toString());
      case 'turn.started':
        _sessionState = ConversationSessionState.running;
        _appendAssistantShell(streaming: true);
        notifyListeners();
      case 'turn.completed':
        _finishTurn();
      case 'turn.cancelled':
        _finishTurn(cancelled: true);
      case 'agent.event':
        _handleNormalizedAgentEvent(payload);
      default:
        if (kind.endsWith('.approval_requested') ||
            kind.contains('approval')) {
          _handleApproval(payload);
        }
    }
  }

  void _handleNormalizedAgentEvent(Map<String, dynamic> payload) {
    final kind = payload['kind']?.toString() ?? '';
    final data = (payload['data'] as Map?)?.cast<String, dynamic>() ?? payload;
    switch (kind) {
      case 'text_delta':
        _appendAssistantText(data['text']?.toString() ?? data['delta']?.toString() ?? '');
      case 'tool_requested':
      case 'tool_started':
        _addToolMessage(
          name: data['name']?.toString() ?? data['tool']?.toString() ?? 'tool',
          input: data['input']?.toString() ?? data['arguments']?.toString(),
          running: kind == 'tool_started',
        );
      case 'tool_output':
        _updateLatestToolOutput(data['output']?.toString() ?? data['text']?.toString());
      case 'tool_completed':
        _completeLatestTool(data['output']?.toString());
      case 'approval_requested':
        _handleApproval(data);
      case 'usage_updated':
        _usage = _usage.merge(_parseUsage(data));
        notifyListeners();
      case 'provider_failed':
        _finishTurn(
          failed: true,
          error: data['detail']?.toString() ?? data['message']?.toString(),
        );
      case 'turn_completed':
        _finishTurn();
      case 'turn_cancelled':
        _finishTurn(cancelled: true);
      default:
        break;
    }
  }

  void _handleAgentLine(Map<String, dynamic> payload) {
    final turnId = (payload['id'] as num?)?.toInt();
    if (turnId != null && _activeTurnId != null && turnId != _activeTurnId) {
      return;
    }
    final message = payload['message'];
    if (message is String) {
      _appendAssistantText(message);
      return;
    }
    if (message is! Map) return;
    final map = message.cast<String, dynamic>();
    _parseStreamJson(map);
  }

  void _parseStreamJson(Map<String, dynamic> map) {
    final type = map['type']?.toString();
    switch (type) {
      case 'assistant':
        final content = map['message']?['content'];
        if (content is List) {
          for (final block in content) {
            if (block is Map) _parseContentBlock(block.cast<String, dynamic>());
          }
        }
      case 'content_block_delta':
      case 'stream_event':
        final delta = map['delta'];
        if (delta is Map) {
          final text = delta['text']?.toString();
          if (text != null && text.isNotEmpty) {
            _appendAssistantText(text);
          }
        }
        final event = map['event'];
        if (event is Map) {
          _parseStreamJson(event.cast<String, dynamic>());
        }
      case 'tool_use':
      case 'tool_call':
        _addToolMessage(
          name: map['name']?.toString() ?? 'tool',
          input: map['input']?.toString() ?? map['arguments']?.toString(),
          running: true,
        );
      case 'tool_result':
        _completeLatestTool(map['output']?.toString() ?? map['content']?.toString());
      case 'result':
        final result = map['result']?.toString();
        if (result != null && result.isNotEmpty) {
          _appendAssistantText(result);
        }
        final sessionId = map['session_id']?.toString();
        if (sessionId != null) _nativeSessionId = sessionId;
        _usage = _usage.merge(_parseUsage(map));
        notifyListeners();
      case 'error':
        _finishTurn(
          failed: true,
          error: map['error']?.toString() ?? map['message']?.toString(),
        );
      default:
        final text = map['text']?.toString() ?? map['content']?.toString();
        if (text != null && text.isNotEmpty) {
          _appendAssistantText(text);
        }
    }
  }

  void _parseContentBlock(Map<String, dynamic> block) {
    final blockType = block['type']?.toString();
    switch (blockType) {
      case 'text':
        _appendAssistantText(block['text']?.toString() ?? '');
      case 'tool_use':
        _addToolMessage(
          name: block['name']?.toString() ?? 'tool',
          input: block['input']?.toString(),
          running: true,
        );
      default:
        break;
    }
  }

  void _handleAgentExit(Map<String, dynamic> payload) {
    final turnId = (payload['id'] as num?)?.toInt();
    if (turnId != null && _activeTurnId != null && turnId != _activeTurnId) {
      return;
    }
    final exitCode = (payload['exitCode'] as num?)?.toInt();
    _finishTurn(
      failed: exitCode != null && exitCode != 0,
      error: exitCode != null && exitCode != 0
          ? 'Agent exited with code $exitCode'
          : null,
    );
  }

  void _handleApproval(Map<String, dynamic> payload) {
    final approval = PendingApproval(
      id: payload['id']?.toString() ?? _nextId('approval'),
      title: payload['title']?.toString() ??
          payload['summary']?.toString() ??
          payload['tool']?.toString() ??
          'Approval required',
      detail: payload['detail']?.toString() ??
          payload['description']?.toString() ??
          payload['method']?.toString(),
    );
    if (_pendingApprovals.any((item) => item.id == approval.id)) return;
    _pendingApprovals.add(approval);
    _sessionState = ConversationSessionState.waitingApproval;
    notifyListeners();
  }

  Future<void> decideApproval(PendingApproval approval, String decision) async {
    if (_core.status != CoreConnectionStatus.connected) return;
    try {
      await _core.request(
        'approval.decide',
        params: {'approvalId': approval.id, 'decision': decision},
      );
      _pendingApprovals.removeWhere((item) => item.id == approval.id);
      if (_pendingApprovals.isEmpty &&
          _sessionState == ConversationSessionState.waitingApproval) {
        _sessionState = ConversationSessionState.idle;
      }
      notifyListeners();
    } on Object catch (error) {
      _sessionError = error.toString();
      notifyListeners();
    }
  }

  void _appendAssistantShell({required bool streaming}) {
    if (_messages.isNotEmpty &&
        _messages.last.role == ConversationMessageRole.assistant &&
        _messages.last.streaming) {
      return;
    }
    _messages.add(
      ConversationMessage(
        id: _nextId('assistant'),
        role: ConversationMessageRole.assistant,
        streaming: streaming,
        timestamp: DateTime.now(),
      ),
    );
  }

  void _appendAssistantText(String chunk) {
    if (chunk.isEmpty) return;
    if (_messages.isEmpty ||
        _messages.last.role != ConversationMessageRole.assistant ||
        !_messages.last.streaming) {
      _appendAssistantShell(streaming: true);
    }
    final last = _messages.removeLast();
    _messages.add(last.copyWith(text: last.text + chunk));
    _sessionState = ConversationSessionState.running;
    notifyListeners();
  }

  void _addToolMessage({
    required String name,
    String? input,
    bool running = false,
  }) {
    _messages.add(
      ConversationMessage(
        id: _nextId('tool'),
        role: ConversationMessageRole.tool,
        toolName: name,
        toolInput: input,
        streaming: running,
        timestamp: DateTime.now(),
      ),
    );
    notifyListeners();
  }

  void _updateLatestToolOutput(String? output) {
    if (output == null || output.isEmpty) return;
    for (var index = _messages.length - 1; index >= 0; index--) {
      final message = _messages[index];
      if (message.role == ConversationMessageRole.tool) {
        _messages[index] = message.copyWith(toolOutput: output, streaming: true);
        notifyListeners();
        return;
      }
    }
  }

  void _completeLatestTool(String? output) {
    for (var index = _messages.length - 1; index >= 0; index--) {
      final message = _messages[index];
      if (message.role == ConversationMessageRole.tool && message.streaming) {
        _messages[index] = message.copyWith(
          toolOutput: output ?? message.toolOutput,
          streaming: false,
        );
        notifyListeners();
        return;
      }
    }
  }

  void _finishTurn({
    bool cancelled = false,
    bool failed = false,
    String? error,
  }) {
    if (_messages.isNotEmpty &&
        _messages.last.role == ConversationMessageRole.assistant &&
        _messages.last.streaming) {
      final last = _messages.removeLast();
      _messages.add(last.copyWith(streaming: false));
    }
    _activeTurnId = null;
    _sessionState = switch ((cancelled, failed)) {
      (true, _) => ConversationSessionState.cancelled,
      (_, true) => ConversationSessionState.failed,
      _ => ConversationSessionState.completed,
    };
    if (failed && error != null) {
      _sessionError = error;
      _messages.add(
        ConversationMessage(
          id: _nextId('error'),
          role: ConversationMessageRole.error,
          errorMessage: error,
          timestamp: DateTime.now(),
        ),
      );
    }
    notifyListeners();
  }

  TokenUsage _parseUsage(Map<String, dynamic> data) {
    final usage = (data['usage'] as Map?)?.cast<String, dynamic>() ?? data;
    return TokenUsage(
      inputTokens: (usage['input_tokens'] as num?)?.toInt() ??
          (usage['inputTokens'] as num?)?.toInt() ??
          0,
      outputTokens: (usage['output_tokens'] as num?)?.toInt() ??
          (usage['outputTokens'] as num?)?.toInt() ??
          0,
      costUsd: (usage['cost_usd'] as num?)?.toDouble() ??
          (usage['costUsd'] as num?)?.toDouble(),
    );
  }

  String _eventKind(Map<String, dynamic> event) =>
      event['kind']?.toString() ??
      event['name']?.toString() ??
      event['type']?.toString() ??
      '';

  Map<String, dynamic> _eventPayload(Map<String, dynamic> event) {
    final payload = event['payload'];
    if (payload is Map<String, dynamic>) return payload;
    if (payload is Map) return payload.cast<String, dynamic>();
    return event;
  }

  int _messageCounter = 0;
  String _nextId(String prefix) {
    _messageCounter++;
    return '$prefix-$_messageCounter';
  }

  /// Test hook for replaying core events without a live connection.
  @visibleForTesting
  void ingestEvent(Map<String, dynamic> event) => _onEvent(event);

  @override
  void dispose() {
    unawaited(_events?.cancel());
    _core.removeListener(_handleCoreStatus);
    super.dispose();
  }
}
