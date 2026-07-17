import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_desktop/src/browser/browser.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

void main() {
  group('InMemoryBrowserRepository', () {
    test(
      'manages isolated sessions, tabs, history, viewport, and artifacts',
      () async {
        final repository = InMemoryBrowserRepository();

        var snapshot = await repository.launch();
        final firstProfile = snapshot.session!.profileId;
        snapshot = await repository.navigate('https://example.com/first');
        snapshot = await repository.navigate('https://example.com/second');
        expect(snapshot.session!.activeTab!.canGoBack, isTrue);
        snapshot = await repository.back();
        expect(snapshot.session!.activeTab!.url, 'https://example.com/first');
        snapshot = await repository.forward();
        expect(snapshot.session!.activeTab!.url, 'https://example.com/second');

        snapshot = await repository.newTab(url: 'https://retcon.local');
        expect(snapshot.session!.tabs, hasLength(2));
        snapshot = await repository.setViewport(
          const BrowserViewport(
            width: 390,
            height: 844,
            device: BrowserDevicePreset.mobile,
            deviceScaleFactor: 2,
          ),
        );
        expect(snapshot.session!.viewport.device, BrowserDevicePreset.mobile);
        snapshot = await repository.captureScreenshot(fullPage: true);
        expect(snapshot.artifacts.single.label, 'Full-page screenshot');

        await repository.close();
        snapshot = await repository.launch();
        expect(snapshot.session!.profileId, isNot(firstProfile));
      },
    );

    test('masks secrets, bounds evidence, and records safe actions', () async {
      final repository = InMemoryBrowserRepository();
      await repository.launch();
      var snapshot = await repository.navigate(
        'https://example.com/?token=private&view=summary',
        metadata: const {
          'cookie': 'session=private',
          'headers': {'authorization': 'Bearer private'},
        },
      );

      expect(
        snapshot.session!.previewMetadata.toString(),
        isNot(contains('private')),
      );
      expect(snapshot.evidence.toString(), isNot(contains('private')));
      expect(
        maskBrowserText('Authorization: Bearer private'),
        isNot(contains('private')),
      );

      for (var index = 0; index < 230; index++) {
        snapshot = await repository.performAction(
          const BrowserAutomationAction(
            kind: BrowserActionKind.fill,
            selector: '#password',
            value: 'super-secret',
          ),
        );
      }
      expect(
        snapshot.evidence.length,
        InMemoryBrowserRepository.maxEvidenceEntries,
      );
      expect(snapshot.evidence.last.details['value'], maskedValue);
      expect(snapshot.evidence.toString(), isNot(contains('super-secret')));
    });

    test('tracks takeover intervals and recovers a crashed profile', () async {
      final repository = InMemoryBrowserRepository();
      var snapshot = await repository.launch();
      final profile = snapshot.session!.profileId;
      snapshot = await repository.pauseAutomation(reason: 'Inspect OAuth');
      expect(snapshot.session!.automationPaused, isTrue);
      snapshot = await repository.openHeadedTakeover();
      expect(snapshot.session!.headless, isFalse);
      snapshot = await repository.resumeAutomation();
      expect(snapshot.session!.takeoverHistory.single.endedAt, isNotNull);

      repository.simulateCrash(message: 'Chromium exited');
      snapshot = await repository.recover();
      expect(snapshot.session!.status, BrowserRuntimeStatus.running);
      expect(snapshot.session!.profileId, profile);
      expect(snapshot.session!.recoveryCount, 1);
    });
  });

  test(
    'BrowserController opens dev preview metadata without verification state',
    () async {
      final repository = InMemoryBrowserRepository();
      final controller = BrowserController(repository: repository);
      addTearDown(controller.dispose);
      await controller.load();

      await controller.openPreview(
        const BrowserPreviewRequest(
          url: 'http://127.0.0.1:5173',
          metadata: {'title': 'Vite preview', 'token': 'private'},
        ),
      );

      expect(controller.running, isTrue);
      expect(controller.address, 'http://127.0.0.1:5173');
      expect(controller.session!.previewMetadata['title'], 'Vite preview');
      expect(controller.session!.previewMetadata['token'], maskedValue);
    },
  );

  testWidgets(
    'managed browser panel exposes navigation, evidence, takeover, and recovery',
    (tester) async {
      tester.view.devicePixelRatio = 1;
      tester.view.physicalSize = const Size(1400, 1000);
      addTearDown(tester.view.resetDevicePixelRatio);
      addTearDown(tester.view.resetPhysicalSize);
      final repository = InMemoryBrowserRepository();
      await tester.pumpWidget(
        MaterialApp(
          theme: buildLunaDarkTheme(),
          home: Scaffold(body: BrowserPanel(repository: repository)),
        ),
      );
      await tester.pumpAndSettle();

      expect(
        find.byKey(const Key('browser-not-verification-evidence')),
        findsOneWidget,
      );
      await tester.tap(find.byKey(const Key('browser-launch')));
      await tester.pumpAndSettle();
      expect(find.textContaining('isolated-profile-1'), findsOneWidget);

      await tester.enterText(
        find.byKey(const Key('browser-address')),
        'https://example.com/?token=private',
      );
      await tester.tap(find.byKey(const Key('browser-go')));
      await tester.pumpAndSettle();
      expect(find.textContaining('https://example.com/'), findsWidgets);
      expect(find.textContaining('private'), findsNothing);

      await tester.tap(find.text('Evidence'));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const Key('browser-evidence-network')));
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('browser-evidence-list')), findsOneWidget);

      await tester.tap(find.text('Automation'));
      await tester.pumpAndSettle();
      await tester.ensureVisible(
        find.byKey(const Key('browser-pause-automation')),
      );
      await tester.tap(find.byKey(const Key('browser-pause-automation')));
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('browser-takeover-banner')), findsOneWidget);
      await tester.tap(find.text('Resume').first);
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('browser-takeover-banner')), findsNothing);

      repository.simulateCrash(message: 'Chromium exited');
      await tester.pump();
      expect(find.byKey(const Key('browser-crash-banner')), findsOneWidget);
      await tester.tap(find.byKey(const Key('browser-recover')));
      await tester.pumpAndSettle();
      expect(find.text('Running'), findsOneWidget);
    },
  );
}
