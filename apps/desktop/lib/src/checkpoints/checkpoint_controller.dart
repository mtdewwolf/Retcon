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

/// File change entry from `checkpoint.get`.
class CheckpointFileChange {
  const CheckpointFileChange({
    required this.path,
    required this.changeKind,
    this.beforeArtifactHash,
    this.afterArtifactHash,
  });

  final String path;
  final String changeKind;
  final String? beforeArtifactHash;
  final String? afterArtifactHash;

  factory CheckpointFileChange.fromJson(Map<String, dynamic> json) =>
      CheckpointFileChange(
        path: json['path']?.toString() ?? '',
        changeKind: json['changeKind']?.toString() ?? '',
        beforeArtifactHash: json['beforeArtifactHash']?.toString(),
        afterArtifactHash: json['afterArtifactHash']?.toString(),
      );
}

/// One path in a `checkpoint.preview` response.
class CheckpointPreviewItem {
  const CheckpointPreviewItem({
    required this.path,
    required this.changeKind,
    required this.action,
    required this.conflict,
  });

  final String path;
  final String changeKind;
  final String action;
  final bool conflict;

  factory CheckpointPreviewItem.fromJson(Map<String, dynamic> json) =>
      CheckpointPreviewItem(
        path: json['path']?.toString() ?? '',
        changeKind: json['changeKind']?.toString() ?? '',
        action: json['action']?.toString() ?? '',
        conflict: json['conflict'] == true,
      );
}

/// Detail loaded via `checkpoint.get` plus optional preview.
class CheckpointDetail {
  const CheckpointDetail({
    required this.checkpoint,
    required this.fileChanges,
    this.previewItems = const [],
  });

  final CheckpointItem checkpoint;
  final List<CheckpointFileChange> fileChanges;
  final List<CheckpointPreviewItem> previewItems;
}

/// Outcome of `checkpoint.restore`.
class CheckpointRestoreReport {
  const CheckpointRestoreReport({
    required this.checkpointId,
    required this.restored,
    required this.skipped,
    required this.conflicts,
  });

  final String checkpointId;
  final List<String> restored;
  final List<String> skipped;
  final List<String> conflicts;

  factory CheckpointRestoreReport.fromJson(Map<String, dynamic> json) =>
      CheckpointRestoreReport(
        checkpointId: json['checkpointId']?.toString() ?? '',
        restored: _stringList(json['restored']),
        skipped: _stringList(json['skipped']),
        conflicts: _stringList(json['conflicts']),
      );
}

List<String> _stringList(Object? value) => (value as List<dynamic>? ?? const [])
    .map((item) => item.toString())
    .toList();

String? _eventKind(Map<String, dynamic> event) {
  final nested = event['event'];
  final envelope = nested is Map ? nested.cast<String, dynamic>() : event;
  return envelope['kind']?.toString() ??
      envelope['name']?.toString() ??
      envelope['type']?.toString();
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
  CheckpointDetail? _detail;
  final Set<String> _selectedPaths = {};
  bool _loading = false;
  bool _detailLoading = false;
  bool _mutating = false;
  String? _error;
  String? _detailError;
  CheckpointRestoreReport? _lastRestore;

  List<CheckpointItem> get items => _items;
  CheckpointDetail? get detail => _detail;
  Set<String> get selectedPaths => _selectedPaths;
  bool get loading => _loading;
  bool get detailLoading => _detailLoading;
  bool get mutating => _mutating;
  String? get error => _error;
  String? get detailError => _detailError;
  CheckpointRestoreReport? get lastRestore => _lastRestore;
  String? get selectedId => _detail?.checkpoint.id;

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
      final checkpoints = (result['checkpoints'] as List<dynamic>? ?? const [])
          .cast<Map<String, dynamic>>()
          .map(CheckpointItem.fromJson)
          .toList();
      _items = checkpoints;
      final selected = selectedId;
      if (selected != null && !_items.any((item) => item.id == selected)) {
        _detail = null;
        _selectedPaths.clear();
      }
    } catch (error) {
      _error = error.toString();
    } finally {
      _loading = false;
      notifyListeners();
    }
  }

  Future<void> createManual() async {
    if (core.status != CoreConnectionStatus.connected || _mutating) return;
    _mutating = true;
    _error = null;
    notifyListeners();
    try {
      final result = await core.request(
        'checkpoint.create',
        params: {'root': root, 'kind': 'manual'},
      );
      final created = CheckpointItem.fromJson(result);
      await refresh();
      await select(created.id);
    } catch (error) {
      _error = error.toString();
    } finally {
      _mutating = false;
      notifyListeners();
    }
  }

  Future<void> select(String checkpointId) async {
    if (core.status != CoreConnectionStatus.connected) return;
    if (checkpointId.isEmpty) return;
    _detailLoading = true;
    _detailError = null;
    _lastRestore = null;
    _selectedPaths.clear();
    notifyListeners();
    try {
      final result = await core.request(
        'checkpoint.get',
        params: {'checkpointId': checkpointId},
      );
      final checkpointJson =
          (result['checkpoint'] as Map?)?.cast<String, dynamic>() ?? result;
      final changes = (result['fileChanges'] as List<dynamic>? ?? const [])
          .cast<Map<String, dynamic>>()
          .map(CheckpointFileChange.fromJson)
          .toList();
      _detail = CheckpointDetail(
        checkpoint: CheckpointItem.fromJson(checkpointJson),
        fileChanges: changes,
      );
      _selectedPaths.addAll(changes.map((change) => change.path));
    } catch (error) {
      _detail = null;
      _detailError = error.toString();
    } finally {
      _detailLoading = false;
      notifyListeners();
    }
  }

  Future<void> previewSelected() async {
    final id = selectedId;
    if (id == null || core.status != CoreConnectionStatus.connected) return;
    _detailLoading = true;
    _detailError = null;
    notifyListeners();
    try {
      final result = await core.request(
        'checkpoint.preview',
        params: {'root': root, 'checkpointId': id},
      );
      final items = (result['items'] as List<dynamic>? ?? const [])
          .cast<Map<String, dynamic>>()
          .map(CheckpointPreviewItem.fromJson)
          .toList();
      final current = _detail;
      if (current == null || current.checkpoint.id != id) return;
      _detail = CheckpointDetail(
        checkpoint: current.checkpoint,
        fileChanges: current.fileChanges,
        previewItems: items,
      );
      if (_selectedPaths.isEmpty) {
        _selectedPaths.addAll(items.map((item) => item.path));
      }
    } catch (error) {
      _detailError = error.toString();
    } finally {
      _detailLoading = false;
      notifyListeners();
    }
  }

  void togglePath(String path) {
    if (_selectedPaths.contains(path)) {
      _selectedPaths.remove(path);
    } else {
      _selectedPaths.add(path);
    }
    notifyListeners();
  }

  void selectAllPaths(Iterable<String> paths) {
    _selectedPaths
      ..clear()
      ..addAll(paths);
    notifyListeners();
  }

  void clearPathSelection() {
    _selectedPaths.clear();
    notifyListeners();
  }

  Future<CheckpointRestoreReport?> restoreSelected({bool force = false}) async {
    final id = selectedId;
    if (id == null ||
        _selectedPaths.isEmpty ||
        core.status != CoreConnectionStatus.connected ||
        _mutating) {
      return null;
    }
    _mutating = true;
    _detailError = null;
    notifyListeners();
    try {
      final result = await core.request(
        'checkpoint.restore',
        params: {
          'root': root,
          'checkpointId': id,
          'paths': _selectedPaths.toList()..sort(),
          'force': force,
        },
      );
      final report = CheckpointRestoreReport.fromJson(result);
      _lastRestore = report;
      await refresh();
      await select(id);
      return report;
    } catch (error) {
      _detailError = error.toString();
      return null;
    } finally {
      _mutating = false;
      notifyListeners();
    }
  }

  void _onEvent(Map<String, dynamic> event) {
    if (_eventKind(event) == 'checkpoint.created') {
      unawaited(refresh());
    }
  }

  @override
  void dispose() {
    unawaited(_events?.cancel());
    super.dispose();
  }
}
