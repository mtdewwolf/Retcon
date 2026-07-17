import 'dart:async';

import 'package:flutter/foundation.dart';

import '../core_client.dart';

/// One checkpoint row loaded from `checkpoint.list`.
class CheckpointItem {
  const CheckpointItem({
    required this.id,
    required this.kind,
    this.turnId,
    this.createdAt,
  });

  final String id;
  final String kind;
  final String? turnId;
  final int? createdAt;

  factory CheckpointItem.fromJson(Map<String, dynamic> json) => CheckpointItem(
    id: json['id']?.toString() ?? '',
    kind: json['kind']?.toString() ?? 'manual',
    turnId: json['turnId']?.toString(),
    createdAt: (json['createdAt'] as num?)?.toInt(),
  );
}

/// Loads checkpoints for the open project through core RPC.
class CheckpointController extends ChangeNotifier {
  CheckpointController(this.core, {required this.root}) {
    _events = core.events.listen(_onEvent);
    unawaited(refresh());
  }

  final CoreClient core;
  final String root;
  StreamSubscription<Map<String, dynamic>>? _events;

  List<CheckpointItem> _items = const [];
  bool _loading = false;
  String? _error;

  List<CheckpointItem> get items => _items;
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
        'checkpoint.list',
        params: {'root': root, 'limit': 50},
      );
      final checkpoints =
          (result['checkpoints'] as List<dynamic>? ?? const [])
              .cast<Map<String, dynamic>>()
              .map(CheckpointItem.fromJson)
              .toList();
      _items = checkpoints;
    } catch (error) {
      _error = error.toString();
    } finally {
      _loading = false;
      notifyListeners();
    }
  }

  void _onEvent(Map<String, dynamic> event) {
    if (event['event']?['type'] == 'checkpoint.created') {
      unawaited(refresh());
    }
  }

  @override
  void dispose() {
    unawaited(_events?.cancel());
    super.dispose();
  }
}
