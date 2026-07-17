import 'dart:async';

import 'package:flutter/foundation.dart';

import 'dev_server_models.dart';
import 'dev_server_repository.dart';

class DevServerController extends ChangeNotifier {
  DevServerController({
    required DevServerRepository repository,
    required this.projectId,
    required this.worktreePath,
    this.maxLogCharacters = 16000,
  }) : _repository = repository {
    _events = repository.events
        .where((event) => event.projectId == projectId)
        .listen(_onEvent);
  }

  final DevServerRepository _repository;
  final String projectId;
  final String worktreePath;
  final int maxLogCharacters;
  StreamSubscription<DevServerEvent>? _events;

  DevServerConfig? config;
  DevServerSnapshot? snapshot;
  String stdout = '';
  String stderr = '';
  bool loading = false;
  bool loaded = false;
  String? error;

  DevServerStatus get status => snapshot?.status ?? DevServerStatus.stopped;
  bool get running => snapshot?.running == true;
  bool get transitioning => snapshot?.transitioning == true;
  bool get canOpenPreview => running && config != null;
  String? get url => config?.url;

  Future<void> load() async {
    loading = true;
    error = null;
    notifyListeners();
    try {
      config = await _repository.detect(
        projectId: projectId,
        worktreePath: worktreePath,
      );
      snapshot =
          await _repository.getSnapshot(projectId) ??
          DevServerSnapshot(config: config!, status: DevServerStatus.stopped);
      loaded = true;
      if (config!.autoStart && !running) await start();
    } on Object catch (caught) {
      error = caught.toString();
    } finally {
      loading = false;
      notifyListeners();
    }
  }

  Future<void> start() async {
    final current = config;
    if (current == null || transitioning || running) return;
    error = null;
    try {
      snapshot = await _repository.start(current);
    } on Object catch (caught) {
      error = caught.toString();
    }
    notifyListeners();
  }

  Future<void> stop() async {
    if (transitioning || status == DevServerStatus.stopped) return;
    error = null;
    try {
      snapshot = await _repository.stop(projectId);
    } on Object catch (caught) {
      error = caught.toString();
    }
    notifyListeners();
  }

  Future<void> restart() async {
    final current = config;
    if (current == null || transitioning) return;
    error = null;
    try {
      snapshot = await _repository.restart(current);
    } on Object catch (caught) {
      error = caught.toString();
    }
    notifyListeners();
  }

  Future<void> openPreview() async {
    final previewUrl = url;
    if (!canOpenPreview || previewUrl == null) return;
    await _repository.openPreview(previewUrl);
  }

  Future<void> startAndOpenPreview() async {
    if (!running) await start();
    if (running) await openPreview();
  }

  Future<void> changePort(int port, {bool restartIfRunning = false}) async {
    final current = config;
    if (current == null || port < 1 || port > 65535) return;
    final wasRunning = running;
    config = await _repository.saveConfig(current.copyWith(port: port));
    snapshot = snapshot?.copyWith(config: config);
    notifyListeners();
    if (restartIfRunning && wasRunning) await restart();
  }

  Future<void> useSuggestedPort() async {
    final port = snapshot?.suggestedPort;
    if (port == null) return;
    await changePort(port);
    await start();
  }

  Future<void> updateStartupCommand(String command) async {
    final current = config;
    if (current == null || command.trim().isEmpty) return;
    config = await _repository.saveConfig(
      current.copyWith(startupCommand: command.trim()),
    );
    snapshot = snapshot?.copyWith(config: config);
    notifyListeners();
  }

  Future<void> setAutoStart(bool value) async {
    final current = config;
    if (current == null) return;
    config = await _repository.saveConfig(current.copyWith(autoStart: value));
    snapshot = snapshot?.copyWith(config: config);
    notifyListeners();
  }

  Future<void> setRequiredForPreview(bool value) async {
    final current = config;
    if (current == null) return;
    config = await _repository.saveConfig(
      current.copyWith(requiredForPreview: value),
    );
    snapshot = snapshot?.copyWith(config: config);
    notifyListeners();
  }

  Future<void> saveEnvironment(
    List<DevServerEnvironmentVariable> environment,
  ) async {
    final current = config;
    if (current == null) return;
    config = await _repository.saveConfig(
      current.copyWith(environment: environment),
    );
    snapshot = snapshot?.copyWith(config: config);
    notifyListeners();
  }

  void clearLogs() {
    stdout = '';
    stderr = '';
    notifyListeners();
  }

  void _onEvent(DevServerEvent event) {
    switch (event) {
      case DevServerChanged(:final snapshot):
        this.snapshot = snapshot;
        config = snapshot.config;
      case DevServerLog(:final text, :final stderr):
        if (stderr) {
          this.stderr = _bounded('${this.stderr}$text');
        } else {
          stdout = _bounded('$stdout$text');
        }
    }
    notifyListeners();
  }

  String _bounded(String value) => value.length <= maxLogCharacters
      ? value
      : '…${value.substring(value.length - maxLogCharacters)}';

  @override
  void dispose() {
    unawaited(_events?.cancel());
    super.dispose();
  }
}
