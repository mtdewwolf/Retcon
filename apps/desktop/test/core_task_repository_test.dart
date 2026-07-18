import 'dart:async';
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_desktop/src/tasks/tasks.dart';

void main() {
  group('CoreTaskRepository', () {
    test('creates tasks through task.create', () async {
      final rpc = FakeTaskRpcClient((method, params) async {
        if (method == 'task.create') {
          expect(params['title'], 'New roadmap item');
          expect(params['projectId'], projectId);
          return {'task': _details(status: 'backlog')};
        }
        throw StateError('Unexpected request: $method');
      });
      final repository = CoreTaskRepository(rpc);

      final created = await repository.createTask(
        'New roadmap item',
        projectId: projectId,
      );

      expect(rpc.calls.single.method, 'task.create');
      expect(created.title, 'Phase 21');
      expect(created.status, TaskStatus.backlog);
    });

    test('loads task details and maps backend wire states', () async {
      final rpc = FakeTaskRpcClient((method, params) async {
        if (method == 'task.list') {
          return {
            'tasks': [
              {'id': taskId, 'title': 'Phase 21'},
            ],
          };
        }
        if (method == 'task.get') return {'task': _details()};
        throw StateError('Unexpected request: $method');
      });
      final repository = CoreTaskRepository(rpc);

      final tasks = await repository.listTasks(projectId: projectId);

      expect(rpc.calls.first.params, {'projectId': projectId});
      expect(tasks, hasLength(1));
      expect(tasks.single.status, TaskStatus.review);
      expect(tasks.single.phase, 21);
      expect(tasks.single.planApproved, isTrue);
      expect(tasks.single.assignee, 'Agent A');
      expect(tasks.single.steps.single.status, PlanStepStatus.inProgress);
      expect(tasks.single.steps.single.assignee, 'Desktop agent');
      expect(tasks.single.steps.single.evidence, ['old check']);
      expect(tasks.single.criteria.single.status, CriterionStatus.pending);
    });

    test('persists plan and acceptance before requesting completion', () async {
      var getCount = 0;
      final rpc = FakeTaskRpcClient((method, params) async {
        switch (method) {
          case 'task.get':
            getCount++;
            return {
              'task': getCount == 1
                  ? _details()
                  : _details(
                      status: 'completed',
                      stepStatus: 'completed',
                      criterionStatus: 'passed',
                      criterionEvidence: const ['flutter test'],
                    ),
            };
          case 'task.update':
            return {'task': _details()};
          case 'task.plan.replace':
            return const {'steps': []};
          case 'task.acceptance.evidence':
          case 'task.acceptance.evaluate':
            return const {'acceptanceCriterion': {}};
          case 'task.status.set':
            return {'task': _details(status: 'completed')};
          default:
            throw StateError('Unexpected request: $method');
        }
      });
      final repository = CoreTaskRepository(rpc);
      final current = await repository.saveTask(
        RoadmapTask(
          id: taskId,
          title: 'Phase 21 integrated',
          phase: 21,
          status: TaskStatus.complete,
          assignee: 'Agent A',
          planApproved: true,
          tags: const ['desktop', 'verified'],
          steps: const [
            PlanStep(
              id: stepId,
              title: 'Build the adapter',
              status: PlanStepStatus.complete,
              assignee: 'Desktop agent',
              evidence: ['old check', 'flutter test'],
            ),
          ],
          criteria: const [
            AcceptanceCriterion(
              id: criterionId,
              title: 'Tests pass',
              status: CriterionStatus.passed,
              evidence: ['flutter test'],
            ),
          ],
        ),
        projectId: projectId,
      );

      expect(current.status, TaskStatus.complete);
      expect(rpc.calls.map((call) => call.method), [
        'task.get',
        'task.update',
        'task.plan.replace',
        'task.acceptance.evidence',
        'task.acceptance.evaluate',
        'task.status.set',
        'task.get',
      ]);
      final update = rpc.calls.firstWhere(
        (call) => call.method == 'task.update',
      );
      expect(update.params['projectId'], projectId);
      final description = jsonDecode(update.params['description'] as String);
      expect(description['_retconDesktop']['planApproved'], isTrue);
      expect(description['_retconDesktop']['tags'], ['desktop', 'verified']);

      final plan = rpc.calls.firstWhere(
        (call) => call.method == 'task.plan.replace',
      );
      final step = (plan.params['steps'] as List).single as Map;
      expect(step['id'], stepId);
      expect(step['status'], 'completed');
      final stepDescription = jsonDecode(step['description'] as String);
      expect(stepDescription['_retconDesktop']['assignee'], 'Desktop agent');
      expect(stepDescription['_retconDesktop']['evidence'], [
        'old check',
        'flutter test',
      ]);

      final status = rpc.calls.firstWhere(
        (call) => call.method == 'task.status.set',
      );
      expect(status.params['status'], 'completed');
      final evaluation = rpc.calls.firstWhere(
        (call) => call.method == 'task.acceptance.evaluate',
      );
      expect(evaluation.params, {'criterionId': criterionId, 'passed': true});
    });

    test('controller refreshes when task.changed is emitted', () async {
      final rpc = FakeTaskRpcClient((method, params) async {
        if (method == 'task.list') return const {'tasks': []};
        throw StateError('Unexpected request: $method');
      });
      final controller = TaskBoardController(
        repository: CoreTaskRepository(rpc),
      );
      addTearDown(controller.dispose);
      await controller.refresh();

      rpc.emit({
        'kind': 'task.changed',
        'payload': {'taskId': taskId},
      });
      await Future<void>.delayed(Duration.zero);
      await Future<void>.delayed(Duration.zero);

      expect(
        rpc.calls.where((call) => call.method == 'task.list'),
        hasLength(2),
      );
    });
  });
}

const taskId = '11111111-1111-4111-8111-111111111111';
const projectId = '22222222-2222-4222-8222-222222222222';
const stepId = '33333333-3333-4333-8333-333333333333';
const criterionId = '44444444-4444-4444-8444-444444444444';

Map<String, dynamic> _details({
  String status = 'review',
  String stepStatus = 'in_progress',
  String criterionStatus = 'pending',
  List<Object?> criterionEvidence = const [],
}) => {
  'task': {
    'id': taskId,
    'projectId': projectId,
    'title': 'Phase 21',
    'description': jsonEncode({
      'description': 'User-authored description',
      '_retconDesktop': {
        'phase': 21,
        'planApproved': true,
        'tags': ['desktop'],
      },
    }),
    'agent': 'Agent A',
    'status': status,
  },
  'steps': [
    {
      'id': stepId,
      'title': 'Build the adapter',
      'status': stepStatus,
      'description': jsonEncode({
        '_retconDesktop': {
          'assignee': 'Desktop agent',
          'evidence': ['old check'],
        },
      }),
    },
  ],
  'acceptanceCriteria': [
    {
      'id': criterionId,
      'description': 'Tests pass',
      'status': criterionStatus,
      'evidence': criterionEvidence,
      'sortOrder': 0,
      'isRequired': true,
    },
  ],
};

class RpcCall {
  const RpcCall(this.method, this.params);
  final String method;
  final Map<String, dynamic> params;
}

class FakeTaskRpcClient implements TaskRpcClient {
  FakeTaskRpcClient(this._handler);

  final Future<Map<String, dynamic>> Function(
    String method,
    Map<String, dynamic> params,
  )
  _handler;
  final calls = <RpcCall>[];
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
}
