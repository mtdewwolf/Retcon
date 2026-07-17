import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_desktop/src/dev_server/dev_server.dart';

const projectId = '11111111-1111-4111-8111-111111111111';
const configId = '22222222-2222-4222-8222-222222222222';
const instanceId = '33333333-3333-4333-8333-333333333333';

void main() {
  group('CoreDevServerRepository', () {
    late FakeDevServerRpc rpc;
    late CoreDevServerRepository repository;

    setUp(() {
      rpc = FakeDevServerRpc();
      repository = CoreDevServerRepository(rpc);
    });

    tearDown(() async {
      await rpc.dispose();
    });

    test(
      'maps redacted config, status, history, logs, and preview metadata',
      () async {
        rpc.instances = [_runningInstance];

        final config = await repository.detect(
          projectId: projectId,
          worktreePath: r'C:\work\app',
        );
        final snapshot = await repository.getSnapshot(projectId);
        final logs = await repository.loadLogs(projectId);
        final preview = await repository.openPreview(snapshot!);

        expect(config.id, configId);
        expect(config.framework, 'Vite');
        expect(config.environment, hasLength(1));
        expect(config.environment.single.key, 'API_TOKEN');
        expect(config.environment.single.value, isEmpty);
        expect(config.environment.single.secret, isTrue);
        expect(snapshot.instanceId, instanceId);
        expect(snapshot.status, DevServerStatus.running);
        expect(snapshot.previewMetadata, {'title': 'Preview'});
        expect(snapshot.history.single.kind, 'started');
        expect(logs, 'ready\n');
        expect(preview?.url, 'http://127.0.0.1:5173');
        expect(preview?.metadata, {'title': 'Preview'});
      },
    );

    test('uses dedicated lifecycle, port, and auto-start methods', () async {
      final config = await repository.detect(
        projectId: projectId,
        worktreePath: r'C:\work\app',
      );

      final started = await repository.start(config);
      final changed = await repository.changePort(config, 5180);
      final automatic = await repository.setAutoStart(config, true);
      final restarted = await repository.restart(config);
      final stopped = await repository.stop(projectId);

      expect(started.status, DevServerStatus.running);
      expect(changed.port, 5180);
      expect(automatic.autoStart, isTrue);
      expect(restarted.status, DevServerStatus.running);
      expect(stopped.status, DevServerStatus.stopped);
      expect(
        rpc.calls.map((call) => call.method),
        containsAllInOrder([
          'devServer.start',
          'devServer.port.change',
          'devServer.autoStart.set',
          'devServer.restart',
          'devServer.stop',
        ]),
      );
      expect(
        rpc.calls
            .firstWhere((call) => call.method == 'devServer.port.change')
            .params,
        {'configId': configId, 'port': 5180},
      );
    });

    test(
      'configures a detected command when no durable config exists',
      () async {
        rpc.configs = [];

        final config = await repository.detect(
          projectId: projectId,
          worktreePath: r'C:\work\app',
        );

        expect(config.id, configId);
        expect(config.startupCommand, 'npm run dev');
        expect(rpc.calls.map((call) => call.method), [
          'devServer.list',
          'devServer.detect',
          'devServer.configure',
        ]);
      },
    );

    test('does not overwrite redacted secrets during config edits', () async {
      final config = await repository.detect(
        projectId: projectId,
        worktreePath: r'C:\work\app',
      );

      await expectLater(
        repository.saveConfig(config.copyWith(startupCommand: 'npm run serve')),
        throwsA(isA<StateError>()),
      );
      expect(
        rpc.calls.where((call) => call.method == 'devServer.configure'),
        isEmpty,
      );

      final saved = await repository.saveConfig(
        config.copyWith(
          startupCommand: 'npm run serve',
          environment: const [
            DevServerEnvironmentVariable(
              key: 'API_TOKEN',
              value: 'new-secret',
              secret: true,
            ),
          ],
        ),
      );
      final configure = rpc.calls.lastWhere(
        (call) => call.method == 'devServer.configure',
      );
      expect(configure.params['environment'], {'API_TOKEN': 'new-secret'});
      expect(saved.environment.single.value, 'new-secret');
    });

    test(
      'refreshes lifecycle event state and emits only new log content',
      () async {
        rpc.instances = [_runningInstance];
        final future = repository.events.take(2).toList();

        rpc.emit('dev_server.started', {'projectId': projectId});
        final events = await future;

        expect(events.first, isA<DevServerChanged>());
        expect(
          (events.first as DevServerChanged).snapshot.history,
          hasLength(1),
        );
        expect(events.last, isA<DevServerLog>());
        expect((events.last as DevServerLog).text, 'ready\n');
        expect(
          rpc.calls.map((call) => call.method),
          containsAll([
            'devServer.list',
            'devServer.history',
            'devServer.logs',
          ]),
        );
      },
    );
  });
}

class RpcCall {
  const RpcCall(this.method, this.params);
  final String method;
  final Map<String, dynamic> params;
}

class FakeDevServerRpc implements DevServerRpcClient {
  final _events = StreamController<Map<String, dynamic>>.broadcast();
  final calls = <RpcCall>[];
  List<Map<String, dynamic>> configs = [_config];
  List<Map<String, dynamic>> instances = [];

  @override
  Stream<Map<String, dynamic>> get events => _events.stream;

  void emit(String kind, Map<String, dynamic> payload) {
    _events.add({
      'event': {'kind': kind, 'payload': payload},
    });
  }

  Future<void> dispose() => _events.close();

  @override
  Future<Map<String, dynamic>> request(
    String method, {
    Map<String, dynamic> params = const {},
  }) async {
    calls.add(RpcCall(method, params));
    switch (method) {
      case 'devServer.list':
        return {'configs': configs, 'instances': instances};
      case 'devServer.detect':
        return const {
          'candidates': [
            {
              'projectId': projectId,
              'name': 'web',
              'command': 'npm run dev',
              'cwd': r'C:\work\app',
              'source': 'package.json',
            },
          ],
        };
      case 'devServer.configure':
        configs = [
          {
            ..._config,
            'command': params['command'],
            'environmentKeys':
                (params['environment'] as Map?)?.keys.toList() ??
                const <String>[],
          },
        ];
        return {'config': configs.single};
      case 'devServer.start':
        instances = [_runningInstance];
        return const {'instance': _runningInstance};
      case 'devServer.restart':
        instances = [_runningInstance];
        return const {'instance': _runningInstance};
      case 'devServer.stop':
        instances = [_stoppedInstance];
        return const {'instance': _stoppedInstance};
      case 'devServer.port.change':
        return const {'lease': <String, dynamic>{}};
      case 'devServer.autoStart.set':
        return {
          'config': {..._config, 'autoStart': params['enabled']},
        };
      case 'devServer.history':
        return const {
          'events': [
            {'kind': 'started', 'actor': 'local_user', 'createdAt': 2000},
          ],
        };
      case 'devServer.logs':
        return const {
          'log': {
            'artifactHash': 'abc',
            'retainedBytes': 6,
            'originalBytes': 6,
            'truncated': false,
            'text': 'ready\n',
          },
        };
      case 'devServer.openPreview':
        return const {
          'instanceId': instanceId,
          'status': 'running',
          'url': 'http://127.0.0.1:5173',
          'port': 5173,
          'preview': {'title': 'Preview'},
        };
      case 'devServer.status':
        return const {'instance': _runningInstance};
      default:
        throw StateError('Unexpected method $method');
    }
  }
}

const _config = <String, dynamic>{
  'id': configId,
  'projectId': projectId,
  'worktreeId': null,
  'name': 'Vite',
  'command': 'npm run dev',
  'cwd': r'C:\work\app',
  'host': '127.0.0.1',
  'preferredPort': 5173,
  'autoStart': false,
  'envAllowlist': ['API_TOKEN'],
  'environmentKeys': ['API_TOKEN'],
};

const _runningInstance = <String, dynamic>{
  'id': instanceId,
  'configId': configId,
  'projectId': projectId,
  'worktreeId': null,
  'taskId': null,
  'port': 5173,
  'status': 'running',
  'pid': 42,
  'url': 'http://127.0.0.1:5173',
  'preview': {'title': 'Preview'},
  'logArtifactHash': 'abc',
  'failure': null,
  'createdAt': 1000,
  'startedAt': 2000,
  'stoppedAt': null,
};

const _stoppedInstance = <String, dynamic>{
  'id': instanceId,
  'configId': configId,
  'projectId': projectId,
  'worktreeId': null,
  'taskId': null,
  'port': 5173,
  'status': 'stopped',
  'pid': null,
  'url': 'http://127.0.0.1:5173',
  'preview': {'title': 'Preview'},
  'logArtifactHash': 'abc',
  'failure': null,
  'createdAt': 1000,
  'startedAt': 2000,
  'stoppedAt': 3000,
};
