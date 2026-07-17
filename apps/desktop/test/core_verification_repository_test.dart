import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_desktop/src/verification/verification.dart';

void main() {
  group('CoreVerificationRepository', () {
    test('maps command configuration and the durable run lifecycle', () async {
      final rpc = FakeVerificationRpcClient(
        (method, params) async {
          switch (method) {
            case 'verification.commands.list':
              return {
                'commands': [_command()],
              };
            case 'verification.commands.configure':
              return {
                'commands': [
                  for (final command in params['commands'] as List)
                    {
                      ...(command as Map).cast<String, dynamic>(),
                      'id': command['id'] ?? commandId,
                      'projectId': projectId,
                      'updatedAt': 1000,
                    },
                ],
              };
            case 'verification.create':
              return {'verification': _details(runId, status: 'queued')};
            case 'verification.start':
              final id = params['runId'] as String;
              return {'verification': _details(id, status: 'running')};
            case 'verification.cancel':
              return {'verification': _details(runId, status: 'cancelled')};
            case 'verification.rerun':
              return {'verification': _details(rerunId, status: 'queued')};
            case 'verification.list':
              return {
                'verifications': [_details(runId)['run']],
              };
            case 'verification.get':
              return {
                'verification': _details(
                  params['runId'] as String,
                  status: 'passed',
                ),
              };
            case 'verification.history':
              return {
                'events': [
                  {
                    'kind': 'finished',
                    'actor': 'local_user',
                    'createdAt': 3000,
                  },
                ],
              };
            case 'verification.report':
              return {
                'report': {
                  'runId': runId,
                  'status': 'passed',
                  'filesChanged': ['lib/app.dart'],
                  'approvals': {'total': 2, 'approved': 2, 'denied': 0},
                  'cost': {
                    'estimatedMicros': 120000,
                    'actualMicros': 100000,
                    'currency': 'USD',
                  },
                  'limitations': ['no browser verification configured'],
                },
              };
            default:
              throw StateError('Unexpected request: $method');
          }
        },
        artifacts: {
          stdoutHash: List.filled(70000, 'x').join(),
          stderrHash: 'warning',
        },
      );
      final repository = CoreVerificationRepository(rpc);

      final commands = await repository.detectCommands(projectId: projectId);
      expect(commands.single.id, commandId);
      expect(commands.single.kind, 'test');
      expect(commands.single.timeout, const Duration(seconds: 30));

      final gates = await repository.loadGates(taskId, projectId: projectId);
      expect(gates.single.required, isTrue);
      expect(gates.single.enabled, isTrue);

      final saved = await repository.saveCommandOverride(
        commands.single,
        projectId: projectId,
      );
      expect(saved.source, CommandSource.override);

      final configured = await repository.saveGates(taskId, [
        gates.single.copyWith(required: false),
      ], projectId: projectId);
      expect(configured.single.id, commandId);
      expect(configured.single.required, isFalse);
      final configure = rpc.calls.lastWhere(
        (call) => call.method == 'verification.commands.configure',
      );
      final configuredWire = (configure.params['commands'] as List).single;
      expect(configuredWire['id'], commandId);

      final run = await repository.startRun(taskId, configured);
      expect(run.status, VerificationRunStatus.running);
      expect(
        rpc.calls.map((call) => call.method),
        containsAllInOrder(['verification.create', 'verification.start']),
      );

      final history = await repository.listHistory(taskId);
      expect(history.single.status, VerificationRunStatus.passed);
      expect(history.single.gates.single.gateId, commandId);
      expect(history.single.gates.single.tests.passed, 1);
      expect(
        history.single.gates.single.fileLinks.single.path,
        'test/app.dart',
      );
      expect(history.single.gates.single.stdout.length, lessThan(70000));
      expect(history.single.gates.single.stderr, 'warning');
      expect(history.single.auditTrail.single.kind, 'finished');
      expect(rpc.artifactReadLimits, everyElement(64 * 1024));

      final report = await repository.loadReport(runId);
      expect(report?.filesChanged, ['lib/app.dart']);
      expect(report?.approvalsApproved, 2);
      expect(report?.actualCostMicros, 100000);
      expect(report?.limitations, hasLength(1));

      final rerun = await repository.rerunRun(runId);
      expect(rerun.id, rerunId);
      expect(
        rpc.calls.map((call) => call.method),
        containsAllInOrder(['verification.rerun', 'verification.start']),
      );
      await repository.cancelRun(rerunId);
      expect(rpc.calls.last.method, 'verification.cancel');
    });

    test('verification and task events refresh a live run snapshot', () async {
      final rpc = FakeVerificationRpcClient((method, params) async {
        if (method == 'verification.get') {
          return {
            'verification': _details(
              params['runId'] as String,
              status: 'passed',
            ),
          };
        }
        throw StateError('Unexpected request: $method');
      }, artifacts: {stdoutHash: 'complete', stderrHash: ''});
      final repository = CoreVerificationRepository(rpc);

      final first = repository.events.first;
      rpc.emit({
        'kind': 'verification.gate.recorded',
        'payload': {'runId': runId, 'taskId': taskId},
      });
      final event = await first;

      expect(event, isA<RunUpdated>());
      expect((event as RunUpdated).run.status, VerificationRunStatus.passed);
      expect(event.run.gates.single.stdout, 'complete');

      final second = repository.events.first;
      rpc.emit({
        'kind': 'task.changed',
        'payload': {'verificationRunId': runId, 'taskId': taskId},
      });
      expect(await second, isA<RunUpdated>());
      expect(
        rpc.calls.where((call) => call.method == 'verification.get'),
        hasLength(2),
      );
    });
  });
}

const projectId = '11111111-1111-4111-8111-111111111111';
const taskId = '22222222-2222-4222-8222-222222222222';
const commandId = '33333333-3333-4333-8333-333333333333';
const runGateId = '44444444-4444-4444-8444-444444444444';
const runId = '55555555-5555-4555-8555-555555555555';
const rerunId = '66666666-6666-4666-8666-666666666666';
const stdoutHash =
    'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';
const stderrHash =
    'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb';

Map<String, dynamic> _command() => {
  'id': commandId,
  'projectId': projectId,
  'key': 'unit',
  'kind': 'test',
  'command': 'flutter test',
  'cwd': 'apps/desktop',
  'required': true,
  'enabled': true,
  'timeoutMs': 30000,
  'updatedAt': 1000,
};

Map<String, dynamic> _details(String id, {String status = 'passed'}) => {
  'run': {
    'id': id,
    'taskId': taskId,
    'projectId': projectId,
    'status': status,
    'triggerKind': 'manual',
    'createdAt': 1000,
    'startedAt': 1200,
    if (status != 'running' && status != 'queued') 'completedAt': 2500,
  },
  'gates': [
    {
      'id': runGateId,
      'runId': id,
      'commandId': commandId,
      'key': 'unit',
      'kind': 'test',
      'command': 'flutter test',
      'required': true,
      'status': status == 'queued' ? 'pending' : status,
      'startedAt': 1300,
      if (status != 'running' && status != 'queued') 'completedAt': 2300,
    },
  ],
  'results': [
    {
      'id': '77777777-7777-4777-8777-777777777777',
      'runId': id,
      'gateId': runGateId,
      'name': 'renders app',
      'status': 'passed',
      'durationMs': 500,
      'filePath': 'test/app.dart',
      'line': 12,
    },
  ],
  'artifacts': [
    {'gateId': runGateId, 'kind': 'stdout', 'hash': stdoutHash},
    {'gateId': runGateId, 'kind': 'stderr', 'hash': stderrHash},
  ],
};

class RpcCall {
  const RpcCall(this.method, this.params);
  final String method;
  final Map<String, dynamic> params;
}

class FakeVerificationRpcClient implements VerificationRpcClient {
  FakeVerificationRpcClient(this._handler, {this.artifacts = const {}});

  final Future<Map<String, dynamic>> Function(
    String method,
    Map<String, dynamic> params,
  )
  _handler;
  final Map<String, String> artifacts;
  final calls = <RpcCall>[];
  final artifactReadLimits = <int>[];
  final _events = StreamController<Map<String, dynamic>>.broadcast();

  @override
  Stream<Map<String, dynamic>> get events => _events.stream;

  void emit(Map<String, dynamic> event) => _events.add(event);

  @override
  Future<Map<String, dynamic>> request(
    String method, {
    Map<String, dynamic> params = const {},
  }) {
    calls.add(RpcCall(method, params));
    return _handler(method, params);
  }

  @override
  Future<String?> readArtifact(String hash, {required int maxBytes}) async {
    artifactReadLimits.add(maxBytes);
    final content = artifacts[hash];
    if (content == null) return null;
    return content.length <= maxBytes
        ? content
        : '${content.substring(0, maxBytes)}\n… [artifact truncated]';
  }
}
