import 'dart:async';

import 'package:flutter/foundation.dart';

import '../core_client.dart';

/// One approval row loaded from `approval.list`.
class ApprovalItem {
  const ApprovalItem({
    required this.id,
    required this.sessionId,
    required this.status,
    required this.title,
    this.detail,
    this.method,
    this.category,
    this.requestedAt,
  });

  final String id;
  final String sessionId;
  final String status;
  final String title;
  final String? detail;
  final String? method;
  final String? category;
  final int? requestedAt;

  factory ApprovalItem.fromJson(Map<String, dynamic> json) {
    final request = json['request'] as Map<String, dynamic>? ?? const {};
    return ApprovalItem(
      id: json['id']?.toString() ?? '',
      sessionId: json['sessionId']?.toString() ?? '',
      status: json['status']?.toString() ?? 'pending',
      title: request['summary']?.toString() ??
          request['title']?.toString() ??
          request['method']?.toString() ??
          'Approval required',
      detail: request['method']?.toString(),
      method: request['method']?.toString(),
      category: request['category']?.toString(),
      requestedAt: (json['requestedAt'] as num?)?.toInt(),
    );
  }
}

/// Loads and resolves approvals through core RPC.
class ApprovalController extends ChangeNotifier {
  ApprovalController(this.core) {
    _events = core.events.listen(_onEvent);
    unawaited(refresh());
  }

  final CoreClient core;
  StreamSubscription<Map<String, dynamic>>? _events;

  List<ApprovalItem> _items = const [];
  int _pendingCount = 0;
  bool _loading = false;
  String? _error;

  List<ApprovalItem> get items => _items;
  int get pendingCount => _pendingCount;
  bool get loading => _loading;
  String? get error => _error;

  Future<void> refresh() async {
    if (core.status != CoreConnectionStatus.connected) {
      return;
    }
    _loading = true;
    _error = null;
    notifyListeners();
    try {
      final result = await core.request(
        'approval.list',
        params: const {'status': 'pending', 'limit': 100},
      );
      final approvals = (result['approvals'] as List<dynamic>? ?? const [])
          .cast<Map<String, dynamic>>()
          .map(ApprovalItem.fromJson)
          .toList();
      _items = approvals;
      _pendingCount =
          (result['pendingCount'] as num?)?.toInt() ?? approvals.length;
    } on Object catch (error) {
      _error = error.toString();
    } finally {
      _loading = false;
      notifyListeners();
    }
  }

  Future<bool> decide(
    String approvalId,
    String decision, {
    String remember = 'once',
    String? projectId,
  }) async {
    try {
      await core.request(
        'approval.decide',
        params: {
          'approvalId': approvalId,
          'decision': decision,
          'remember': remember,
          'projectId': ?projectId,
        },
      );
      await refresh();
      return true;
    } on Object catch (error) {
      _error = error.toString();
      notifyListeners();
      return false;
    }
  }

  void _onEvent(Map<String, dynamic> event) {
    final envelope = event['event'] as Map<String, dynamic>? ?? event;
    final kind = envelope['kind']?.toString() ?? envelope['name']?.toString();
    if (kind == 'approval.requested' ||
        kind == 'approval.decided' ||
        kind == 'permission.rule_created' ||
        kind == 'permission.rule_deleted') {
      unawaited(refresh());
    }
  }

  @override
  void dispose() {
    unawaited(_events?.cancel());
    super.dispose();
  }
}
