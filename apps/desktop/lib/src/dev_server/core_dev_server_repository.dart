import 'dart:async';

import '../core_client.dart';
import 'dev_server_models.dart';
import 'dev_server_repository.dart';

abstract interface class DevServerRpcClient {
  Stream<Map<String, dynamic>> get events;

  Future<Map<String, dynamic>> request(
    String method, {
    Map<String, dynamic> params = const {},
  });
}

class CoreDevServerRpcClient implements DevServerRpcClient {
  CoreDevServerRpcClient(this._core);
  final CoreClient _core;

  @override
  Stream<Map<String, dynamic>> get events => _core.events;

  @override
  Future<Map<String, dynamic>> request(
    String method, {
    Map<String, dynamic> params = const {},
  }) => _core.request(method, params: params);
}

class CoreDevServerRepository implements DevServerRepository {
  CoreDevServerRepository(this._rpc);

  factory CoreDevServerRepository.fromCore(CoreClient core) =>
      CoreDevServerRepository(CoreDevServerRpcClient(core));

  final DevServerRpcClient _rpc;
  final Map<String, DevServerConfig> _configsByProject = {};
  final Map<String, DevServerSnapshot> _snapshotsByProject = {};
  final Map<String, bool> _requiredForPreview = {};
  final Map<String, int> _emittedLogLengths = {};

  @override
  late final Stream<DevServerEvent> events = _rpc.events
      .where(_isDevServerEvent)
      .asyncExpand(_decodeEvent)
      .asBroadcastStream();

  @override
  Future<DevServerConfig> detect({
    required String projectId,
    required String worktreePath,
  }) async {
    final listed = await _rpc.request(
      'devServer.list',
      params: {'projectId': projectId},
    );
    final configs = _maps(listed['configs']);
    if (configs.isNotEmpty) {
      return _rememberConfig(
        _decodeConfig(configs.first, fallbackPath: worktreePath),
      );
    }
    final detected = await _rpc.request(
      'devServer.detect',
      params: {'projectId': projectId, 'rootPath': worktreePath},
    );
    final candidates = _maps(detected['candidates']);
    if (candidates.isEmpty) {
      throw StateError('No development server command was detected.');
    }
    final candidate = candidates.first;
    final configured = await _rpc.request(
      'devServer.configure',
      params: {
        'projectId': projectId,
        'name': _framework(candidate),
        'command': candidate['command']?.toString() ?? '',
        'cwd': candidate['cwd']?.toString() ?? worktreePath,
        'host': '127.0.0.1',
        'preferredPort': 3000,
        'autoStart': false,
        'envAllowlist': const <String>[],
        'environment': const <String, String>{},
      },
    );
    return _rememberConfig(
      _decodeConfig(_map(configured['config']), fallbackPath: worktreePath),
    );
  }

  @override
  Future<DevServerConfig> saveConfig(DevServerConfig config) async {
    final previous = _configsByProject[config.projectId];
    if (previous != null &&
        _sameWireConfig(previous, config) &&
        previous.requiredForPreview != config.requiredForPreview) {
      _requiredForPreview[config.projectId] = config.requiredForPreview;
      return _rememberConfig(config);
    }
    final redacted = config.environment.where(
      (variable) => variable.secret && variable.value.isEmpty,
    );
    if (redacted.isNotEmpty) {
      throw StateError(
        'Re-enter masked environment values before changing server configuration.',
      );
    }
    final response = await _rpc.request(
      'devServer.configure',
      params: _encodeConfig(config),
    );
    final decoded = _decodeConfig(
      _map(response['config']),
      fallbackPath: config.worktreePath,
      submittedEnvironment: config.environment,
    ).copyWith(requiredForPreview: config.requiredForPreview);
    _requiredForPreview[config.projectId] = config.requiredForPreview;
    return _rememberConfig(decoded);
  }

  @override
  Future<DevServerConfig> setAutoStart(
    DevServerConfig config,
    bool enabled,
  ) async {
    final response = await _rpc.request(
      'devServer.autoStart.set',
      params: {'configId': config.id, 'enabled': enabled},
    );
    return _rememberConfig(
      _decodeConfig(
        _map(response['config']),
        fallbackPath: config.worktreePath,
        submittedEnvironment: config.environment,
      ).copyWith(requiredForPreview: config.requiredForPreview),
    );
  }

  @override
  Future<DevServerConfig> changePort(DevServerConfig config, int port) async {
    await _rpc.request(
      'devServer.port.change',
      params: {'configId': config.id, 'port': port},
    );
    return _rememberConfig(config.copyWith(port: port));
  }

  @override
  Future<DevServerSnapshot?> getSnapshot(String projectId) async {
    final response = await _rpc.request(
      'devServer.list',
      params: {'projectId': projectId},
    );
    final configs = _maps(response['configs']);
    final instances = _maps(response['instances']);
    if (configs.isEmpty) return null;
    final instance = instances.firstWhere(
      (item) => _activeStatus(item['status']),
      orElse: () => instances.isEmpty ? <String, dynamic>{} : instances.first,
    );
    final configWire = instance.isEmpty
        ? configs.first
        : configs.firstWhere(
            (item) =>
                item['id']?.toString() == instance['configId']?.toString(),
            orElse: () => configs.first,
          );
    var config = _decodeConfig(configWire);
    if (instance['port'] is num) {
      config = config.copyWith(port: (instance['port'] as num).toInt());
    }
    config = _rememberConfig(config);
    if (instance.isEmpty) {
      final stopped = DevServerSnapshot(
        config: config,
        status: DevServerStatus.stopped,
      );
      _snapshotsByProject[projectId] = stopped;
      return stopped;
    }
    final historyResponse = await _rpc.request(
      'devServer.history',
      params: {'instanceId': instance['id']},
    );
    final snapshot = _decodeSnapshot(
      config,
      instance,
      history: _maps(historyResponse['events']).map(_decodeHistory).toList(),
    );
    _snapshotsByProject[projectId] = snapshot;
    return snapshot;
  }

  @override
  Future<String> loadLogs(String projectId) async {
    final snapshot =
        _snapshotsByProject[projectId] ?? await getSnapshot(projectId);
    if (snapshot == null || snapshot.instanceId.isEmpty) return '';
    final log = await _loadLogs(snapshot.instanceId);
    _emittedLogLengths[snapshot.instanceId] = log.length;
    return log;
  }

  Future<String> _loadLogs(String instanceId) async {
    final response = await _rpc.request(
      'devServer.logs',
      params: {'instanceId': instanceId},
    );
    return _map(response['log'])['text']?.toString() ?? '';
  }

  @override
  Future<DevServerSnapshot> start(DevServerConfig config) async {
    final persisted = config.id.isEmpty ? await saveConfig(config) : config;
    try {
      final response = await _rpc.request(
        'devServer.start',
        params: {'configId': persisted.id},
      );
      final instance = _map(response['instance']);
      final snapshot = _decodeSnapshot(
        persisted.copyWith(port: _int(instance['port'], persisted.port)),
        instance,
      );
      _snapshotsByProject[config.projectId] = snapshot;
      return snapshot;
    } on Object catch (error) {
      final refreshed = await getSnapshot(config.projectId);
      if (refreshed != null && refreshed.status != DevServerStatus.stopped) {
        return refreshed;
      }
      final detail = error.toString().toLowerCase();
      if (detail.contains('port')) {
        final conflict = DevServerSnapshot(
          config: config,
          status: DevServerStatus.portConflict,
          message: error.toString(),
          suggestedPort: config.port + 1,
        );
        _snapshotsByProject[config.projectId] = conflict;
        return conflict;
      }
      rethrow;
    }
  }

  @override
  Future<DevServerSnapshot> stop(String projectId) async {
    final current =
        _snapshotsByProject[projectId] ?? await getSnapshot(projectId);
    if (current == null || current.instanceId.isEmpty) {
      throw StateError('No development server instance is available to stop.');
    }
    final response = await _rpc.request(
      'devServer.stop',
      params: {'instanceId': current.instanceId},
    );
    final snapshot = _decodeSnapshot(
      current.config,
      _map(response['instance']),
      history: current.history,
    );
    _snapshotsByProject[projectId] = snapshot;
    return snapshot;
  }

  @override
  Future<DevServerSnapshot> restart(DevServerConfig config) async {
    final current =
        _snapshotsByProject[config.projectId] ??
        await getSnapshot(config.projectId);
    if (current == null || current.instanceId.isEmpty) return start(config);
    final response = await _rpc.request(
      'devServer.restart',
      params: {'instanceId': current.instanceId},
    );
    final instance = _map(response['instance']);
    final snapshot = _decodeSnapshot(
      config.copyWith(port: _int(instance['port'], config.port)),
      instance,
    );
    _snapshotsByProject[config.projectId] = snapshot;
    return snapshot;
  }

  @override
  Future<DevServerPreviewMetadata?> openPreview(
    DevServerSnapshot snapshot,
  ) async {
    if (snapshot.instanceId.isEmpty) return null;
    final response = await _rpc.request(
      'devServer.openPreview',
      params: {'instanceId': snapshot.instanceId},
    );
    final url = response['url']?.toString();
    if (url == null || url.isEmpty) return null;
    return DevServerPreviewMetadata(
      url: url,
      port: _int(response['port'], snapshot.config.port),
      status: _status(response['status'], startedAt: snapshot.startedAt),
      metadata: _map(response['preview']),
    );
  }

  Stream<DevServerEvent> _decodeEvent(Map<String, dynamic> wire) async* {
    final envelope = _envelope(wire);
    final payload = _map(envelope['payload']);
    var projectId = payload['projectId']?.toString();
    final instanceId = payload['instanceId']?.toString();
    if ((projectId == null || projectId.isEmpty) && instanceId != null) {
      try {
        final status = await _rpc.request(
          'devServer.status',
          params: {'instanceId': instanceId},
        );
        projectId = _map(status['instance'])['projectId']?.toString();
      } on Object {
        return;
      }
    }
    if (projectId == null || projectId.isEmpty) return;
    try {
      final snapshot = await getSnapshot(projectId);
      if (snapshot == null) return;
      yield DevServerChanged(projectId: projectId, snapshot: snapshot);
      final priorLength = _emittedLogLengths[snapshot.instanceId] ?? 0;
      final log = await _loadLogs(snapshot.instanceId);
      if (log.length > priorLength) {
        yield DevServerLog(
          projectId: projectId,
          text: log.substring(priorLength),
        );
      }
      _emittedLogLengths[snapshot.instanceId] = log.length;
    } on Object {
      // A later lifecycle event or the controller poll will retry the refresh.
    }
  }

  DevServerConfig _rememberConfig(DevServerConfig config) {
    final required = _requiredForPreview[config.projectId];
    final remembered = required == null
        ? config
        : config.copyWith(requiredForPreview: required);
    _configsByProject[config.projectId] = remembered;
    return remembered;
  }
}

DevServerConfig _decodeConfig(
  Map<String, dynamic> json, {
  String fallbackPath = '',
  List<DevServerEnvironmentVariable>? submittedEnvironment,
}) {
  final submitted = {
    for (final variable in submittedEnvironment ?? const [])
      variable.key: variable,
  };
  final keys = (json['environmentKeys'] as List? ?? const [])
      .map((item) => item.toString())
      .toList();
  return DevServerConfig(
    id: json['id']?.toString() ?? '',
    projectId: json['projectId']?.toString() ?? '',
    worktreeId: json['worktreeId']?.toString(),
    framework: json['name']?.toString() ?? 'Custom',
    startupCommand: json['command']?.toString() ?? '',
    host: json['host']?.toString() ?? '127.0.0.1',
    port: _int(json['preferredPort'], 3000),
    worktreePath: json['cwd']?.toString() ?? fallbackPath,
    autoStart: json['autoStart'] == true,
    environment: [
      for (final key in keys)
        submitted[key] ??
            DevServerEnvironmentVariable(key: key, value: '', secret: true),
    ],
  );
}

DevServerSnapshot _decodeSnapshot(
  DevServerConfig config,
  Map<String, dynamic> instance, {
  List<DevServerHistoryEntry> history = const [],
}) => DevServerSnapshot(
  instanceId: instance['id']?.toString() ?? '',
  config: config,
  status: _status(instance['status'], startedAt: _date(instance['startedAt'])),
  startedAt: _date(instance['startedAt']),
  stoppedAt: _date(instance['stoppedAt']),
  message: instance['failure']?.toString(),
  history: history,
  previewMetadata: _map(instance['preview']),
);

DevServerHistoryEntry _decodeHistory(Map<String, dynamic> json) =>
    DevServerHistoryEntry(
      kind: json['kind']?.toString() ?? 'updated',
      actor: json['actor']?.toString() ?? 'system',
      createdAt: _date(json['createdAt']) ?? DateTime.now(),
    );

Map<String, dynamic> _encodeConfig(DevServerConfig config) {
  final result = <String, dynamic>{
    'projectId': config.projectId,
    'name': config.framework,
    'command': config.startupCommand,
    'cwd': config.worktreePath,
    'host': config.host,
    'preferredPort': config.port,
    'autoStart': config.autoStart,
    'envAllowlist': [for (final variable in config.environment) variable.key],
    'environment': {
      for (final variable in config.environment) variable.key: variable.value,
    },
  };
  if (_uuid.hasMatch(config.id)) result['id'] = config.id;
  final worktreeId = config.worktreeId;
  if (worktreeId != null && _uuid.hasMatch(worktreeId)) {
    result['worktreeId'] = worktreeId;
  }
  return result;
}

bool _sameWireConfig(DevServerConfig left, DevServerConfig right) =>
    left.id == right.id &&
    left.framework == right.framework &&
    left.startupCommand == right.startupCommand &&
    left.host == right.host &&
    left.port == right.port &&
    left.worktreePath == right.worktreePath &&
    left.autoStart == right.autoStart &&
    left.environment.length == right.environment.length &&
    left.environment.indexed.every((entry) {
      final other = right.environment[entry.$1];
      return entry.$2.key == other.key &&
          entry.$2.value == other.value &&
          entry.$2.secret == other.secret;
    });

String _framework(Map<String, dynamic> candidate) {
  final source = candidate['source']?.toString();
  return switch (source) {
    'package.json' => 'Node.js',
    'Cargo.toml' => 'Rust',
    'manage.py' => 'Django',
    _ => candidate['name']?.toString() ?? 'Custom',
  };
}

DevServerStatus _status(Object? value, {DateTime? startedAt}) =>
    switch (value?.toString()) {
      'starting' => DevServerStatus.starting,
      'running' => DevServerStatus.running,
      'stopping' => DevServerStatus.stopping,
      'failed' when startedAt != null => DevServerStatus.crashed,
      'failed' => DevServerStatus.startupFailed,
      _ => DevServerStatus.stopped,
    };

bool _activeStatus(Object? value) =>
    value == 'starting' || value == 'running' || value == 'stopping';

bool _isDevServerEvent(Map<String, dynamic> wire) {
  final kind = _envelope(wire)['kind']?.toString() ?? '';
  return kind.startsWith('dev_server.');
}

Map<String, dynamic> _envelope(Map<String, dynamic> wire) {
  final nested = wire['event'];
  return nested is Map ? nested.cast<String, dynamic>() : wire;
}

Map<String, dynamic> _map(Object? value) =>
    value is Map ? value.cast<String, dynamic>() : <String, dynamic>{};

List<Map<String, dynamic>> _maps(Object? value) => (value as List? ?? const [])
    .whereType<Map>()
    .map((item) => item.cast<String, dynamic>())
    .toList();

int _int(Object? value, int fallback) => (value as num?)?.toInt() ?? fallback;

DateTime? _date(Object? value) {
  final milliseconds = (value as num?)?.toInt();
  return milliseconds == null
      ? null
      : DateTime.fromMillisecondsSinceEpoch(milliseconds);
}

final _uuid = RegExp(
  r'^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-5][0-9a-fA-F]{3}-'
  r'[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$',
);
