import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_desktop/src/browser/browser.dart';

void main() {
  test(
    'CoreBrowserRepository maps supported service lifecycle and evidence',
    () async {
      final rpc = FakeBrowserRpc();
      final repository = CoreBrowserRepository(
        rpc,
        serviceDir: r'C:\browser-service',
      );
      addTearDown(repository.dispose);

      var snapshot = await repository.launch();
      expect(snapshot.session!.profileId, 'retcon-browser-profile');
      snapshot = await repository.navigate(
        'https://example.com/?token=private',
        metadata: const {'authorization': 'Bearer private'},
      );
      expect(snapshot.session!.activeTab!.title, 'Example');
      expect(
        snapshot.session!.previewMetadata.toString(),
        isNot(contains('private')),
      );
      expect(snapshot.evidence, hasLength(2));

      snapshot = await repository.captureScreenshot();
      expect(snapshot.artifacts, hasLength(1));
      await repository.performAction(
        const BrowserAutomationAction(
          kind: BrowserActionKind.fill,
          selector: '#password',
          value: 'private',
        ),
      );
      expect(repository.capabilities.multipleTabs, isFalse);
      await expectLater(repository.newTab(), throwsUnsupportedError);
      expect(
        rpc.calls.map((call) => call.method),
        containsAllInOrder([
          'browser.startService',
          'browser.call',
          'browser.call',
          'browser.call',
        ]),
      );
    },
  );

  test(
    'CoreBrowserRepository maps browser service exit to crash state',
    () async {
      final rpc = FakeBrowserRpc();
      final repository = CoreBrowserRepository(
        rpc,
        serviceDir: r'C:\browser-service',
      );
      addTearDown(repository.dispose);
      await repository.launch();
      final crashed = repository.events.firstWhere(
        (snapshot) => snapshot.session?.status == BrowserRuntimeStatus.crashed,
      );

      rpc.emit('browser.serviceExited', const {});
      final snapshot = await crashed;

      expect(snapshot.session!.automationPaused, isTrue);
      expect(snapshot.session!.crashMessage, contains('exited'));
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

  @override
  Stream<Map<String, dynamic>> get events => _events.stream;

  void emit(String kind, Map<String, dynamic> payload) {
    _events.add({
      'event': {'kind': kind, 'payload': payload},
    });
  }

  @override
  Future<Map<String, dynamic>> request(
    String method, {
    Map<String, dynamic> params = const {},
  }) async {
    calls.add(RpcCall(method, params));
    if (method == 'browser.startService' || method == 'browser.stopService') {
      return const {};
    }
    final inner = params['method'];
    return switch (inner) {
      'browser.launch' => const {
        'launched': true,
        'profile': r'C:\Temp\retcon-browser-profile',
      },
      'browser.navigate' => const {
        'url': 'https://example.com/?token=private',
        'title': 'Example',
        'status': 200,
      },
      'browser.logs' => const {
        'console': [
          {
            'type': 'log',
            'text': 'ready token=private',
            'timestamp': '2026-01-01T00:00:00Z',
          },
        ],
        'network': [
          {'method': 'GET', 'url': 'https://example.com/?token=private'},
        ],
      },
      'browser.screenshot' => {
        'path': (params['params'] as Map)['path'],
        'fullPage': false,
      },
      'browser.action' => const {'completed': true},
      'browser.status' => const {'running': true, 'url': 'https://example.com'},
      'browser.close' => const {'closed': true},
      _ => throw StateError('Unexpected browser call: $inner'),
    };
  }
}
