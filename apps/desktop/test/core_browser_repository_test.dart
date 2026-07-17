import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_desktop/src/browser/browser.dart';

const projectId = '11111111-1111-4111-8111-111111111111';
const sessionId = '22222222-2222-4222-8222-222222222222';
const profileId = '33333333-3333-4333-8333-333333333333';
const tabId = '44444444-4444-4444-8444-444444444444';
const secondTabId = '55555555-5555-4555-8555-555555555555';
const artifactHash =
    'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';

void main() {
  test(
    'maps durable lifecycle, tabs, navigation, evidence, actions, and takeover',
    () async {
      final rpc = FakeBrowserRpc();
      final repository = CoreBrowserRepository(rpc, projectId: projectId);
      addTearDown(repository.dispose);

      var snapshot = await repository.launch(
        devServerInstanceId: '66666666-6666-4666-8666-666666666666',
      );
      expect(snapshot.session!.profileId, profileId);
      expect(snapshot.session!.activeTab!.id, tabId);
      expect(repository.capabilities.multipleTabs, isTrue);
      expect(repository.capabilities.stopLoading, isFalse);

      snapshot = await repository.navigate(
        'http://127.0.0.1:5173/?token=private',
        metadata: const {'authorization': 'Bearer private'},
      );
      expect(snapshot.session!.activeTab!.title, 'Preview');
      expect(
        snapshot.session!.previewMetadata.toString(),
        isNot(contains('private')),
      );

      snapshot = await repository.newTab(url: 'http://127.0.0.1:5173/second');
      expect(snapshot.session!.tabs, hasLength(2));
      await repository.back();
      await repository.forward();
      await repository.reload();

      snapshot = await repository.captureScreenshot();
      expect(snapshot.artifacts.single.metadata['artifactHash'], artifactHash);
      expect(snapshot.screenshotPath, 'C:/artifacts/$artifactHash');
      await repository.refreshEvidence();
      expect(
        snapshot.evidence.any(
          (entry) => entry.kind == BrowserEvidenceKind.screenshots,
        ),
        isTrue,
      );

      await repository.performAction(
        const BrowserAutomationAction(
          kind: BrowserActionKind.fill,
          selector: '#password',
          value: 'private',
        ),
      );
      snapshot = await repository.pauseAutomation(reason: 'Inspect preview');
      expect(snapshot.session!.automationPaused, isTrue);
      expect(snapshot.session!.takeoverHistory, isNotEmpty);
      snapshot = await repository.resumeAutomation();
      expect(snapshot.session!.automationPaused, isFalse);

      expect(
        rpc.calls.map((call) => call.method),
        containsAll([
          'browser.session.start',
          'browser.session.status',
          'browser.session.history',
          'browser.tab.open',
          'browser.navigate',
          'browser.back',
          'browser.forward',
          'browser.reload',
          'browser.observation.screenshot',
          'browser.observation.list',
          'browser.automation.action',
          'browser.takeover.start',
          'browser.takeover.stop',
        ]),
      );
    },
  );

  test(
    'loads active durable session and refreshes matching Core events',
    () async {
      final rpc = FakeBrowserRpc()..running = true;
      final repository = CoreBrowserRepository(rpc, projectId: projectId);
      addTearDown(repository.dispose);

      final loaded = await repository.load();
      expect(loaded.session!.id, sessionId);
      final refreshed = repository.events.firstWhere(
        (snapshot) => snapshot.session?.status == BrowserRuntimeStatus.crashed,
      );
      rpc.status = 'orphaned';
      rpc.emit('browser.session_stopped', {
        'projectId': projectId,
        'sessionId': sessionId,
      });

      expect((await refreshed).session!.status, BrowserRuntimeStatus.crashed);
    },
  );
}

class RpcCall {
  const RpcCall(this.method, this.params);
  final String method;
  final Map<String, dynamic> params;
}

class FakeBrowserRpc implements BrowserRpcClient {
  final _events = StreamController<Map<String, dynamic>>.broadcast();
  final calls = <RpcCall>[];
  bool running = false;
  String status = 'running';
  bool takeover = false;
  final tabs = <Map<String, dynamic>>[_tab(tabId, 'about:blank', 'New tab')];
  final observations = <Map<String, dynamic>>[];
  final history = <Map<String, dynamic>>[];

  @override
  Stream<Map<String, dynamic>> get events => _events.stream;

  void emit(String kind, Map<String, dynamic> payload) {
    _events.add({
      'event': {'kind': kind, 'payload': payload},
    });
  }

  @override
  String? artifactPath(String hash) => 'C:/artifacts/$hash';

  @override
  Future<String?> readArtifact(String hash, {required int maxBytes}) async =>
      '{"token":"[REDACTED]"}';

  @override
  Future<Map<String, dynamic>> request(
    String method, {
    Map<String, dynamic> params = const {},
  }) async {
    calls.add(RpcCall(method, params));
    switch (method) {
      case 'browser.session.list':
        return {
          'sessions': running ? [_session()] : <Object>[],
        };
      case 'browser.session.start':
        running = true;
        return {'session': _session(), 'initialTab': tabs.first};
      case 'browser.session.status':
        return {
          'session': _session(),
          'tabs': tabs,
          'takeover': takeover
              ? {
                  'id': '77777777-7777-4777-8777-777777777777',
                  'reason': 'Inspect preview',
                  'startedAt': 2000,
                }
              : null,
        };
      case 'browser.session.history':
        return {'events': history};
      case 'browser.observation.list':
        return {
          'observations': observations,
          'console': const [
            {'type': 'log', 'text': 'ready token=[REDACTED]'},
          ],
          'network': const [
            {'method': 'GET', 'url': 'http://127.0.0.1:5173'},
          ],
        };
      case 'browser.tab.open':
        final tab = _tab(
          secondTabId,
          params['url']?.toString() ?? 'about:blank',
          'Second',
        );
        tabs.add(tab);
        return {'result': tab, 'artifacts': <Object>[], 'tab': tab};
      case 'browser.tab.close':
        tabs.removeWhere((tab) => tab['id'] == params['tabId']);
        return const {'result': {}, 'artifacts': [], 'tab': null};
      case 'browser.tab.activate':
        return const {'result': {}, 'artifacts': [], 'tab': null};
      case 'browser.navigate':
        final index = tabs.indexWhere((tab) => tab['id'] == params['tabId']);
        tabs[index] = _tab(
          tabs[index]['id']!.toString(),
          params['url']!.toString(),
          'Preview',
        );
        return {
          'result': tabs[index],
          'artifacts': <Object>[],
          'tab': tabs[index],
        };
      case 'browser.observation.screenshot':
        final observation = {
          'id': '88888888-8888-4888-8888-888888888888',
          'browserSessionId': sessionId,
          'tabId': params['tabId'],
          'kind': 'screenshot',
          'artifactHash': artifactHash,
          'mimeType': 'image/png',
          'sizeBytes': 100,
          'metadata': {'fullPage': params['fullPage']},
          'createdAt': 3000,
        };
        observations.add(observation);
        return {
          'result': const {},
          'artifacts': [observation],
          'tab': null,
        };
      case 'browser.observation.logs':
      case 'browser.observation.snapshot':
      case 'browser.back':
      case 'browser.forward':
      case 'browser.reload':
      case 'browser.automation.action':
        return const {'result': {}, 'artifacts': [], 'tab': null};
      case 'browser.takeover.start':
        takeover = true;
        history.add({
          'kind': 'takeover_started',
          'actor': 'local_user',
          'payload': {'reason': params['reason']},
          'createdAt': 2000,
        });
        return const {'takeover': {}};
      case 'browser.takeover.stop':
        takeover = false;
        history.add(const {
          'kind': 'takeover_stopped',
          'actor': 'local_user',
          'payload': {'reason': 'released'},
          'createdAt': 2500,
        });
        return const {'takeover': {}};
      case 'browser.session.stop':
        running = false;
        status = 'stopped';
        return {'session': _session()};
      default:
        throw StateError('Unexpected method $method');
    }
  }

  Map<String, dynamic> _session() => {
    'id': sessionId,
    'projectId': projectId,
    'profileId': profileId,
    'status': status,
    'networkPolicy': 'loopback',
    'devServerInstanceId': '66666666-6666-4666-8666-666666666666',
    'startedAt': 1000,
    'failure': status == 'orphaned' ? 'Browser service exited.' : null,
  };
}

Map<String, dynamic> _tab(String id, String url, String title) => {
  'id': id,
  'browserSessionId': sessionId,
  'serviceTabId': 'service-$id',
  'url': url,
  'title': title,
  'status': 'open',
  'createdAt': 1000,
  'updatedAt': 1000,
};
