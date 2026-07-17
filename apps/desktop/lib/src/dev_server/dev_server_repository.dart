import 'dart:async';

import 'dev_server_models.dart';

abstract interface class DevServerRepository {
  Stream<DevServerEvent> get events;

  Future<DevServerConfig> detect({
    required String projectId,
    required String worktreePath,
  });
  Future<DevServerConfig> saveConfig(DevServerConfig config);
  Future<DevServerConfig> setAutoStart(DevServerConfig config, bool enabled);
  Future<DevServerConfig> changePort(DevServerConfig config, int port);
  Future<DevServerSnapshot?> getSnapshot(String projectId);
  Future<String> loadLogs(String projectId);
  Future<DevServerSnapshot> start(DevServerConfig config);
  Future<DevServerSnapshot> stop(String projectId);
  Future<DevServerSnapshot> restart(DevServerConfig config);
  Future<DevServerPreviewMetadata?> openPreview(DevServerSnapshot snapshot);
}

class InMemoryDevServerRepository implements DevServerRepository {
  InMemoryDevServerRepository({
    Map<String, DevServerConfig> configs = const {},
    Map<String, DevServerSnapshot> snapshots = const {},
    Set<int> occupiedPorts = const {},
    this.failStartup = false,
  }) : _configs = {...configs},
       _snapshots = {...snapshots},
       _occupiedPorts = {...occupiedPorts};

  factory InMemoryDevServerRepository.demo() => InMemoryDevServerRepository();

  final bool failStartup;
  final Map<String, DevServerConfig> _configs;
  final Map<String, DevServerSnapshot> _snapshots;
  final Set<int> _occupiedPorts;
  int _nextInstance = 1;
  final Map<String, String> _logs = {};
  final _events = StreamController<DevServerEvent>.broadcast(sync: true);
  final List<String> openedPreviews = [];

  @override
  Stream<DevServerEvent> get events => _events.stream;

  @override
  Future<DevServerConfig> detect({
    required String projectId,
    required String worktreePath,
  }) async =>
      _configs[projectId] ??
      DevServerConfig(
        projectId: projectId,
        framework: 'Next.js',
        startupCommand: 'npm run dev -- --port {port}',
        port: 3000,
        worktreePath: worktreePath,
      );

  @override
  Future<DevServerConfig> saveConfig(DevServerConfig config) async {
    _configs[config.projectId] = config;
    final current = _snapshots[config.projectId];
    if (current != null) {
      _setSnapshot(current.copyWith(config: config));
    }
    return config;
  }

  @override
  Future<DevServerConfig> setAutoStart(DevServerConfig config, bool enabled) =>
      saveConfig(config.copyWith(autoStart: enabled));

  @override
  Future<DevServerConfig> changePort(DevServerConfig config, int port) =>
      saveConfig(config.copyWith(port: port));

  @override
  Future<DevServerSnapshot?> getSnapshot(String projectId) async =>
      _snapshots[projectId];

  @override
  Future<String> loadLogs(String projectId) async => _logs[projectId] ?? '';

  @override
  Future<DevServerSnapshot> start(DevServerConfig config) async {
    _configs[config.projectId] = config;
    _setSnapshot(
      DevServerSnapshot(config: config, status: DevServerStatus.starting),
    );
    _log(config.projectId, '> ${_resolvedCommand(config)}\n');
    if (_occupiedPorts.contains(config.port)) {
      final alternate = _alternatePort(config.port);
      _log(
        config.projectId,
        'Port ${config.port} is already in use.\n',
        stderr: true,
      );
      return _setSnapshot(
        DevServerSnapshot(
          config: config,
          status: DevServerStatus.portConflict,
          message: 'Port ${config.port} is already in use.',
          suggestedPort: alternate,
        ),
      );
    }
    if (failStartup || config.startupCommand.trim().isEmpty) {
      _log(config.projectId, 'Server failed during startup.\n', stderr: true);
      return _setSnapshot(
        DevServerSnapshot(
          config: config,
          status: DevServerStatus.startupFailed,
          message: 'The startup command exited before the server was ready.',
        ),
      );
    }
    _log(config.projectId, 'Ready on ${config.url}\n');
    return _setSnapshot(
      DevServerSnapshot(
        config: config,
        status: DevServerStatus.running,
        instanceId:
            '00000000-0000-4000-8000-${(_nextInstance++).toString().padLeft(12, '0')}',
        startedAt: DateTime.now(),
      ),
    );
  }

  @override
  Future<DevServerSnapshot> stop(String projectId) async {
    final current =
        _snapshots[projectId] ??
        DevServerSnapshot(
          config: await detect(projectId: projectId, worktreePath: ''),
          status: DevServerStatus.stopped,
        );
    _setSnapshot(current.copyWith(status: DevServerStatus.stopping));
    _log(projectId, 'Server stopped.\n');
    return _setSnapshot(
      DevServerSnapshot(
        config: current.config,
        status: DevServerStatus.stopped,
        stoppedAt: DateTime.now(),
      ),
    );
  }

  @override
  Future<DevServerSnapshot> restart(DevServerConfig config) async {
    if (_snapshots[config.projectId]?.running == true) {
      await stop(config.projectId);
    }
    return start(config);
  }

  @override
  Future<DevServerPreviewMetadata?> openPreview(
    DevServerSnapshot snapshot,
  ) async {
    openedPreviews.add(snapshot.config.url);
    return DevServerPreviewMetadata(
      url: snapshot.config.url,
      port: snapshot.config.port,
      status: snapshot.status,
      metadata: {
        ...snapshot.previewMetadata,
        if (snapshot.instanceId.isNotEmpty)
          'devServerInstanceId': snapshot.instanceId,
      },
    );
  }

  DevServerSnapshot simulateCrash(String projectId, {String? message}) {
    final current = _snapshots[projectId];
    if (current == null) {
      throw StateError('No dev server exists for $projectId.');
    }
    _log(
      projectId,
      '${message ?? 'Server process exited unexpectedly.'}\n',
      stderr: true,
    );
    return _setSnapshot(
      DevServerSnapshot(
        config: current.config,
        status: DevServerStatus.crashed,
        stoppedAt: DateTime.now(),
        message: message ?? 'Server process exited unexpectedly.',
      ),
    );
  }

  DevServerSnapshot _setSnapshot(DevServerSnapshot snapshot) {
    _snapshots[snapshot.config.projectId] = snapshot;
    _events.add(
      DevServerChanged(
        projectId: snapshot.config.projectId,
        snapshot: snapshot,
      ),
    );
    return snapshot;
  }

  void _log(String projectId, String text, {bool stderr = false}) {
    _logs[projectId] = '${_logs[projectId] ?? ''}$text';
    _events.add(DevServerLog(projectId: projectId, text: text, stderr: stderr));
  }

  int _alternatePort(int port) {
    var candidate = port + 1;
    while (_occupiedPorts.contains(candidate)) {
      candidate++;
    }
    return candidate;
  }

  String _resolvedCommand(DevServerConfig config) =>
      config.startupCommand.replaceAll('{port}', config.port.toString());
}
