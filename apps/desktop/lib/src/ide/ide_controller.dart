import 'dart:async';

import 'package:flutter/foundation.dart';

import 'ide_models.dart';
import 'ide_repository.dart';

class IdeController extends ChangeNotifier {
  IdeController(this.repository);
  final IdeRepository repository;

  List<IdeDescriptor> ides = const [];
  String? preferredIdeId;
  bool loading = false;
  bool launching = false;
  Object? error;
  String? notice;

  IdeDescriptor? get preferred {
    for (final ide in ides) {
      if (ide.id == preferredIdeId) return ide;
    }
    return null;
  }

  bool get supported => ides.any((ide) => ide.available);

  Future<void> load() async {
    loading = true;
    error = null;
    notifyListeners();
    try {
      final results = await Future.wait<Object>([
        repository.detect(),
        repository.configuration(),
      ]);
      ides = results[0] as List<IdeDescriptor>;
      preferredIdeId = (results[1] as IdeConfiguration).preferredIdeId;
      if (preferredIdeId == null ||
          !ides.any((ide) => ide.id == preferredIdeId && ide.available)) {
        preferredIdeId = ides.where((ide) => ide.available).firstOrNull?.id;
      }
    } catch (value) {
      error = value;
    } finally {
      loading = false;
      notifyListeners();
    }
  }

  Future<void> select(String? ideId) async {
    loading = true;
    error = null;
    notifyListeners();
    try {
      preferredIdeId = (await repository.updatePreferred(ideId)).preferredIdeId;
      notice = preferredIdeId == null
          ? 'External editor preference cleared.'
          : 'Preferred editor updated.';
    } catch (value) {
      error = value;
    } finally {
      loading = false;
      notifyListeners();
    }
  }

  Future<void> openProject(String path) =>
      _launch(() => repository.openProject(path, ideId: preferredIdeId));
  Future<void> openWorktree(String path) =>
      _launch(() => repository.openWorktree(path, ideId: preferredIdeId));
  Future<void> openFile(
    String workspacePath,
    String path, {
    int? line,
    int? column,
  }) => _launch(
    () => repository.openFile(
      workspacePath: workspacePath,
      path: path,
      ideId: preferredIdeId,
      line: line,
      column: column,
    ),
  );
  Future<void> openDiff(
    String workspacePath,
    String leftPath,
    String rightPath,
  ) => _launch(
    () => repository.openDiff(
      workspacePath: workspacePath,
      leftPath: leftPath,
      rightPath: rightPath,
      ideId: preferredIdeId,
    ),
  );
  Future<void> openTerminal(String workspacePath, {String? path}) => _launch(
    () => repository.openTerminalLocation(
      workspacePath: workspacePath,
      path: path,
      ideId: preferredIdeId,
    ),
  );

  Future<void> _launch(Future<IdeLaunchResult> Function() action) async {
    launching = true;
    error = null;
    notice = null;
    notifyListeners();
    try {
      final result = await action();
      notice = result.launched
          ? 'Opened in ${_name(result.ideId)}.'
          : 'The editor could not be opened.';
    } catch (value) {
      error = value;
    } finally {
      launching = false;
      notifyListeners();
    }
  }

  String _name(String id) {
    for (final ide in ides) {
      if (ide.id == id) return ide.name;
    }
    return 'external editor';
  }
}

class SyncedFileController extends ChangeNotifier {
  SyncedFileController({
    required this.repository,
    required this.root,
    required this.path,
    Stream<Map<String, dynamic>>? events,
  }) {
    _events = events?.listen(_onEvent);
  }

  final SyncedFileRepository repository;
  final String root;
  final String path;
  StreamSubscription<Map<String, dynamic>>? _events;
  Timer? _refreshTimer;

  SyncedFile? file;
  SyncedFile? externalVersion;
  String draft = '';
  bool loading = false;
  bool saving = false;
  bool conflict = false;
  bool externalChangePending = false;
  Object? error;

  bool get dirty => file != null && draft != (file!.content ?? '');
  bool get editable => file != null && !file!.binary && !file!.truncated;

  Future<void> load({bool preserveDraft = false}) async {
    loading = true;
    error = null;
    notifyListeners();
    try {
      final loaded = await repository.read(
        root: root,
        path: path,
        ifNoneMatch: preserveDraft ? null : file?.revision,
        limit: 512 * 1024,
      );
      if (loaded.notModified) {
        externalChangePending = false;
        return;
      }
      file = loaded;
      if (!preserveDraft) draft = loaded.content ?? '';
      conflict = false;
      externalVersion = null;
      externalChangePending = false;
    } catch (value) {
      error = value;
    } finally {
      loading = false;
      notifyListeners();
    }
  }

  void edit(String value) {
    draft = value;
    notifyListeners();
  }

  Future<void> save() async {
    final current = file;
    if (current == null || !editable || !dirty || saving) return;
    saving = true;
    error = null;
    notifyListeners();
    try {
      final result = await repository.write(
        root: root,
        path: path,
        content: draft,
        ifMatch: current.revision,
      );
      file = SyncedFile(
        path: result.path,
        revision: result.revision,
        content: draft,
        size: result.size,
        truncated: false,
        binary: false,
        language: current.language,
      );
      conflict = false;
      externalVersion = null;
      externalChangePending = false;
    } on FileRevisionConflict catch (value) {
      conflict = true;
      error = value;
      externalVersion = await repository.read(
        root: root,
        path: path,
        limit: 512 * 1024,
      );
    } catch (value) {
      error = value;
    } finally {
      saving = false;
      notifyListeners();
    }
  }

  void useExternalVersion() {
    final external = externalVersion;
    if (external == null) return;
    file = external;
    draft = external.content ?? '';
    conflict = false;
    externalVersion = null;
    externalChangePending = false;
    error = null;
    notifyListeners();
  }

  void keepDraftOnLatestRevision() {
    final external = externalVersion;
    if (external == null) return;
    file = external;
    conflict = false;
    externalVersion = null;
    externalChangePending = false;
    error = null;
    notifyListeners();
  }

  void _onEvent(Map<String, dynamic> event) {
    final envelope = (event['event'] as Map?)?.cast<String, dynamic>() ?? event;
    final kind = envelope['kind']?.toString() ?? envelope['type']?.toString();
    if (kind != 'file.changed' && kind != 'file.updated') return;
    final payload =
        (envelope['payload'] as Map?)?.cast<String, dynamic>() ?? envelope;
    if (payload['path']?.toString() != path) return;
    if (dirty || saving) {
      externalChangePending = true;
      notifyListeners();
      return;
    }
    _refreshTimer?.cancel();
    _refreshTimer = Timer(
      const Duration(milliseconds: 150),
      () => unawaited(load()),
    );
  }

  @override
  void dispose() {
    _refreshTimer?.cancel();
    unawaited(_events?.cancel());
    super.dispose();
  }
}
