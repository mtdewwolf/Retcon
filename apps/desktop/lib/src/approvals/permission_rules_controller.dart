import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';

import '../core_client.dart';

/// One permission rule from `permission.rules.list`.
class PermissionRuleItem {
  const PermissionRuleItem({
    required this.id,
    required this.scope,
    required this.effect,
    required this.matcher,
    this.projectId,
    this.createdAt,
    this.expiresAt,
  });

  final String id;
  final String? projectId;
  final String scope;
  final String effect;
  final Map<String, dynamic> matcher;
  final int? createdAt;
  final int? expiresAt;

  String get matcherLabel {
    final method = matcher['method']?.toString();
    if (method != null && method.isNotEmpty) return method;
    final methods = matcher['methods'];
    if (methods is List && methods.isNotEmpty) {
      return methods.map((item) => item.toString()).join(', ');
    }
    final category = matcher['category']?.toString();
    if (category != null && category.isNotEmpty) return 'category:$category';
    return const JsonEncoder.withIndent('  ').convert(matcher);
  }

  factory PermissionRuleItem.fromJson(Map<String, dynamic> json) =>
      PermissionRuleItem(
        id: json['id']?.toString() ?? '',
        projectId: json['projectId']?.toString(),
        scope: json['scope']?.toString() ?? 'rpc',
        effect: json['effect']?.toString() ?? 'allow',
        matcher: (json['matcher'] as Map?)?.cast<String, dynamic>() ?? const {},
        createdAt: (json['createdAt'] as num?)?.toInt(),
        expiresAt: (json['expiresAt'] as num?)?.toInt(),
      );
}

String? _eventKind(Map<String, dynamic> event) {
  final nested = event['event'];
  final envelope = nested is Map
      ? nested.cast<String, dynamic>()
      : event;
  return envelope['kind']?.toString() ??
      envelope['name']?.toString() ??
      envelope['type']?.toString();
}

/// Lists and mutates permission rules through Core RPC.
class PermissionRulesController extends ChangeNotifier {
  PermissionRulesController(this.core, {this.projectId}) {
    _events = core.events.listen(_onEvent);
    unawaited(refresh());
  }

  final CoreClient core;
  String? projectId;
  StreamSubscription<Map<String, dynamic>>? _events;

  List<PermissionRuleItem> _rules = const [];
  bool _loading = false;
  bool _mutating = false;
  String? _error;

  List<PermissionRuleItem> get rules => _rules;
  bool get loading => _loading;
  bool get mutating => _mutating;
  String? get error => _error;

  void updateProjectId(String? value) {
    if (projectId == value) return;
    projectId = value;
    unawaited(refresh());
  }

  Future<void> refresh() async {
    if (core.status != CoreConnectionStatus.connected) return;
    final id = projectId;
    if (id == null || id.isEmpty) {
      _rules = const [];
      _error = 'Open a project to list permission rules.';
      notifyListeners();
      return;
    }
    _loading = true;
    _error = null;
    notifyListeners();
    try {
      final result = await core.request(
        'permission.rules.list',
        params: {'projectId': id},
      );
      _rules = (result['rules'] as List<dynamic>? ?? const [])
          .cast<Map<String, dynamic>>()
          .map(PermissionRuleItem.fromJson)
          .toList();
    } catch (error) {
      _error = error.toString();
    } finally {
      _loading = false;
      notifyListeners();
    }
  }

  Future<void> createRule({
    required String effect,
    required String method,
    bool projectScoped = true,
  }) async {
    final trimmed = method.trim();
    if (trimmed.isEmpty ||
        core.status != CoreConnectionStatus.connected ||
        _mutating) {
      return;
    }
    _mutating = true;
    _error = null;
    notifyListeners();
    try {
      await core.request(
        'permission.rules.create',
        params: {
          'projectId': projectScoped ? projectId : null,
          'scope': 'rpc',
          'effect': effect,
          'matcher': {'method': trimmed},
        },
      );
      await refresh();
    } catch (error) {
      _error = error.toString();
    } finally {
      _mutating = false;
      notifyListeners();
    }
  }

  Future<void> deleteRule(String ruleId) async {
    if (ruleId.isEmpty ||
        core.status != CoreConnectionStatus.connected ||
        _mutating) {
      return;
    }
    _mutating = true;
    _error = null;
    notifyListeners();
    try {
      await core.request(
        'permission.rules.delete',
        params: {'ruleId': ruleId},
      );
      await refresh();
    } catch (error) {
      _error = error.toString();
    } finally {
      _mutating = false;
      notifyListeners();
    }
  }

  void _onEvent(Map<String, dynamic> event) {
    final kind = _eventKind(event);
    if (kind == 'permission.rule_created' ||
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
