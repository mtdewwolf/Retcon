import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_desktop/src/verification/verification.dart';

void main() {
  group('CoreBrowserVerificationRepository', () {
    late FakeBrowserVerificationRpc rpc;
    late CoreBrowserVerificationRepository repository;

    setUp(() {
      rpc = FakeBrowserVerificationRpc();
      repository = CoreBrowserVerificationRepository(rpc, projectId: projectId);
    });

    tearDown(() => rpc.dispose());

    test('isolates settled runner DTO names and normalizes ratios', () async {
      final definition = await repository.loadDefinition(taskId);

      expect(definition, isNotNull);
      expect(rpc.calls.first.method, 'browser.verification.definition.list');
      expect(rpc.calls.first.params['projectId'], projectId);
      expect(definition!.requiredServerId, serverId);
      expect(definition.viewports.single.label, 'Desktop');
      expect(definition.retryCount, 2);
      expect(definition.visualThreshold, closeTo(0.025, 0.00001));
      expect(definition.maskSelectors, ['.clock']);
      expect(definition.steps.single.assertions.single.expected, 'Dashboard');
      final codec = const BrowserVerificationDtoCodec();
      expect(
        codec.decodeStep({'id': 'wait', 'kind': 'wait'}).kind,
        BrowserStepKind.waitFor,
      );
      expect(
        codec.encodeStep(
          const BrowserVerificationStep(
            id: 'wait',
            label: 'Wait for dashboard',
            kind: BrowserStepKind.waitFor,
          ),
        )['kind'],
        'wait',
      );

      await repository.saveDefinition(
        definition.copyWith(visualThreshold: 0.01),
      );
      final save = rpc.calls.firstWhere(
        (call) => call.method == 'browser.verification.definition.update',
      );
      expect(save.params['projectId'], projectId);
      expect(save.params['definitionId'], definitionId);
      expect(save.params['devServerConfigId'], serverId);
      expect(save.params['variants'], hasLength(1));
      expect(save.params['maxRetries'], 2);
      expect(save.params['failOnAccessibility'], isTrue);
      expect((save.params['visualPolicy'] as Map)['maxDiffPixelRatio'], 0.01);

      final run = await repository.startRun(
        definition,
        devServerInstanceId: instanceId,
      );
      expect(rpc.calls.last.method, 'browser.verification.run');
      expect(rpc.calls.last.params['devServerInstanceId'], instanceId);
      expect(run.status, BrowserRunStatus.failed);
      expect(
        run.timeline.map((event) => event.kind),
        contains(BrowserTimelineKind.assertion),
      );
      expect(
        run.visualComparisons.single.method,
        VisualComparisonMethod.perceptual,
      );
      expect(run.visualComparisons.single.difference, 0.02);
      expect(run.visualComparisons.single.threshold, 0.01);
      expect(
        run.visualComparisons.single.current?.localPath,
        contains(currentHash),
      );
      expect(
        run.accessibilityIssues.single.category,
        AccessibilityCategory.labels,
      );
      expect(run.consoleErrors, contains('Uncaught failure'));
      expect(
        run.timeline.map((event) => event.kind),
        contains(BrowserTimelineKind.error),
      );
      final history = await repository.listHistory(taskId);
      expect(history.single.id, runId);
      expect(
        rpc.calls.any((call) => call.method == 'browser.verification.list'),
        isTrue,
      );
    });

    test(
      'durable refresh events reconcile runs and baseline approval',
      () async {
        final event = repository.events.first;
        rpc.emit({
          'event': {
            'kind': 'browser.verification.run.completed',
            'payload': {'runId': runId},
          },
        });
        final decoded = await event;
        expect((decoded as BrowserVerificationRunUpdated).run.id, runId);
        expect(
          rpc.calls.any((call) => call.method == 'browser.verification.get'),
          isTrue,
        );

        final approved = await repository.approveBaseline(runId, comparisonId);
        expect(approved.status, VisualComparisonStatus.approved);
        expect(
          rpc.calls
              .firstWhere(
                (call) =>
                    call.method == 'browser.verification.baseline.approve',
              )
              .params['comparisonId'],
          comparisonId,
        );
        await repository.cancelRun(runId);
        expect(rpc.calls.last.method, 'browser.verification.cancel');
      },
    );
  });
}

class RpcCall {
  const RpcCall(this.method, this.params);
  final String method;
  final Map<String, dynamic> params;
}

class FakeBrowserVerificationRpc implements BrowserVerificationRpcClient {
  final _events = StreamController<Map<String, dynamic>>.broadcast();
  final calls = <RpcCall>[];
  bool approved = false;

  @override
  Stream<Map<String, dynamic>> get events => _events.stream;

  void emit(Map<String, dynamic> event) => _events.add(event);
  Future<void> dispose() => _events.close();

  @override
  String? artifactPath(String hash) => 'C:\\artifacts\\$hash';

  @override
  Future<Map<String, dynamic>> request(
    String method, {
    Map<String, dynamic> params = const {},
  }) async {
    calls.add(RpcCall(method, params));
    if (method == 'browser.verification.baseline.approve') {
      approved = true;
      return {
        'baseline': {
          'id': baselineId,
          'definitionId': definitionId,
          'variantKey': 'desktop',
          'artifactHash': currentHash,
          'sourceRunId': runId,
          'status': 'active',
          'approvedBy': 'local_user',
          'approvedAt': 1784290004000,
        },
      };
    }
    return switch (method) {
      'browser.verification.definition.list' => {
        'definitions': [definitionDto],
      },
      'browser.verification.definition.create' ||
      'browser.verification.definition.update' => {
        'definition': {
          ...definitionDto,
          ...params,
          'id': params['definitionId'] ?? definitionId,
          'projectId': projectId,
        },
      },
      'browser.verification.run' || 'browser.verification.get' => {
        'verification': verificationDto(approved: approved),
      },
      'browser.verification.list' => {
        'runs': [runRecord],
      },
      _ => <String, dynamic>{},
    };
  }
}

const projectId = '00000000-0000-4000-8000-000000000001';
const taskId = '11111111-1111-4111-8111-111111111111';
const definitionId = '22222222-2222-4222-8222-222222222222';
const serverId = '33333333-3333-4333-8333-333333333333';
const instanceId = '33333333-3333-4333-8333-333333333334';
const runId = '44444444-4444-4444-8444-444444444444';
const comparisonId = '55555555-5555-4555-8555-555555555555';
const baselineId = '66666666-6666-4666-8666-666666666666';
const variantId = '77777777-7777-4777-8777-777777777777';
const currentHash =
    'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb';
const diffHash =
    'cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc';

final definitionDto = <String, dynamic>{
  'id': definitionId,
  'projectId': projectId,
  'taskId': taskId,
  'devServerConfigId': serverId,
  'name': 'Dashboard smoke test',
  'targetUrl': 'http://127.0.0.1:3000',
  'variants': [
    {
      'id': variantId,
      'definitionId': definitionId,
      'key': 'desktop',
      'width': 1440,
      'height': 900,
      'deviceScale': 1.0,
      'deviceName': null,
      'sortOrder': 0,
    },
  ],
  'timeoutMs': 30000,
  'maxRetries': 2,
  'visualPolicy': {
    // The codec accepts the earlier DB percent representation at its boundary.
    'maxDiffPixelRatio': 2.5,
    'maskSelectors': ['.clock'],
    'ignoreSelectors': ['video'],
  },
  'status': 'active',
  'required': true,
  'steps': [
    {
      'id': 'navigate',
      'label': 'Open dashboard',
      'kind': 'navigate',
      'target': '/dashboard',
      'timeoutMs': 10000,
      'assertions': [
        {
          'id': 'title',
          'kind': 'text',
          'target': 'h1',
          'expected': 'Dashboard',
          'required': true,
        },
      ],
    },
  ],
  'assertions': [],
};

final runRecord = <String, dynamic>{
  'id': runId,
  'definitionId': definitionId,
  'projectId': projectId,
  'taskId': taskId,
  'devServerInstanceId': instanceId,
  'status': 'failed',
  'attempt': 1,
  'maxAttempts': 3,
  'timeoutMs': 30000,
  'blockingFailures': 1,
  'criticalAccessibility': 1,
  'warningCount': 1,
  'visualDifferences': 1,
  'consoleErrors': 1,
  'summary': {},
  'review': {},
  'createdAt': 1784290000000,
  'startedAt': 1784290000000,
  'completedAt': 1784290004000,
  'updatedAt': 1784290004000,
};

Map<String, dynamic> verificationDto({required bool approved}) => {
  'run': runRecord,
  'definition': definitionDto,
  'events': [
    {
      'id': 1,
      'runId': runId,
      'sequence': 1,
      'kind': 'navigation',
      'severity': 'info',
      'actor': 'browser_verification_runner',
      'payload': {'url': 'http://127.0.0.1:3000/dashboard'},
      'createdAt': 1784290001000,
    },
    {
      'id': 2,
      'runId': runId,
      'sequence': 2,
      'kind': 'assertion',
      'severity': 'info',
      'actor': 'browser_verification_runner',
      'payload': {'message': 'Dashboard heading visible', 'status': 'passed'},
      'createdAt': 1784290002000,
    },
    {
      'id': 3,
      'runId': runId,
      'sequence': 3,
      'kind': 'page_error',
      'severity': 'error',
      'actor': 'browser_verification_runner',
      'payload': {'message': 'Page crashed'},
      'createdAt': 1784290003000,
    },
  ],
  'artifacts': [
    {
      'id': '88888888-8888-4888-8888-888888888888',
      'runId': runId,
      'kind': 'screenshot',
      'hash': currentHash,
      'mimeType': 'image/png',
      'sizeBytes': 1024,
      'metadata': {'variantKey': 'desktop'},
      'createdAt': 1784290003000,
    },
  ],
  'visualComparisons': [
    {
      'id': comparisonId,
      'runId': runId,
      'baselineId': approved ? baselineId : null,
      'variantKey': 'desktop',
      'currentHash': currentHash,
      'differenceHash': diffHash,
      'pixelDifferenceRatio': 0.03,
      'perceptualDifferenceRatio': 0.02,
      'thresholdRatio': 0.01,
      'status': approved ? 'approved' : 'different',
      'ignoreRegions': [
        {
          'id': 'clock',
          'label': 'Clock',
          'x': 10,
          'y': 10,
          'width': 100,
          'height': 30,
        },
      ],
      'createdAt': 1784290003000,
    },
  ],
  'console': [
    {
      'id': 1,
      'runId': runId,
      'sequence': 1,
      'level': 'error',
      'message': 'Uncaught failure',
      'source': 'dashboard.js',
      'createdAt': 1784290003000,
    },
  ],
  'network': [
    {
      'id': 1,
      'runId': runId,
      'sequence': 1,
      'method': 'GET',
      'url': 'http://127.0.0.1:3000/api/dashboard',
      'statusCode': 200,
      'durationMs': 18,
      'createdAt': 1784290002000,
    },
  ],
  'accessibility': [
    {
      'id': '99999999-9999-4999-8999-999999999999',
      'runId': runId,
      'ruleId': 'button-name',
      'severity': 'critical',
      'message': 'Button has no accessible label',
      'selector': '#save',
      'helpUrl': 'https://example.test/button-name',
      'status': 'open',
      'createdAt': 1784290003000,
    },
  ],
};
