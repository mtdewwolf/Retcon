import 'dart:async';
import 'dart:io';

import '../core_client.dart';
import 'browser_models.dart';
import 'browser_repository.dart';

abstract interface class BrowserRpcClient {
  Stream<Map<String, dynamic>> get events;

  Future<Map<String, dynamic>> request(
    String method, {
    Map<String, dynamic> params = const {},
  });
}

class CoreBrowserRpcClient implements BrowserRpcClient {
  CoreBrowserRpcClient(this._core);
  final CoreClient _core;

  @override
  Stream<Map<String, dynamic>> get events => _core.events;

  @override
  Future<Map<String, dynamic>> request(
    String method, {
    Map<String, dynamic> params = const {},
  }) => _core.request(method, params: params);
}

String? resolveBrowserServiceDir({Directory? from}) {
  var dir = from ?? Directory.current;
  for (var i = 0; i < 8; i++) {
    final candidate = Directory(
      '${dir.path}${Platform.pathSeparator}apps'
      '${Platform.pathSeparator}browser-service',
    );
    if (candidate.existsSync()) return candidate.path;
    final parent = dir.parent;
    if (parent.path == dir.path) break;
    dir = parent;
  }
  return null;
}

class CoreBrowserRepository implements BrowserRepository {
  CoreBrowserRepository(this._rpc, {String? serviceDir})
    : _serviceDir = serviceDir ?? resolveBrowserServiceDir() ?? '' {
    _subscription = _rpc.events.listen(_onEvent);
  }

  factory CoreBrowserRepository.fromCore(
    CoreClient core, {
    String? serviceDir,
  }) =>
      CoreBrowserRepository(CoreBrowserRpcClient(core), serviceDir: serviceDir);

  static const maxEvidenceEntries = 200;
  final BrowserRpcClient _rpc;
  final String _serviceDir;
  final _events = StreamController<BrowserSnapshot>.broadcast(sync: true);
  StreamSubscription<Map<String, dynamic>>? _subscription;
  BrowserSnapshot _snapshot = const BrowserSnapshot();
  bool _serviceRunning = false;
  int _sessionSequence = 0;
  int _artifactSequence = 0;

  @override
  BrowserCapabilities get capabilities => const BrowserCapabilities(
    multipleTabs: false,
    historyNavigation: false,
    reloadAndStop: false,
    viewportAndDevice: false,
    accessibility: false,
    performance: false,
    headedTakeover: false,
  );

  @override
  Stream<BrowserSnapshot> get events => _events.stream;

  @override
  Future<BrowserSnapshot> load() async {
    if (!_serviceRunning) return _snapshot;
    return _refreshStatus();
  }

  @override
  Future<BrowserSnapshot> launch() async {
    if (_serviceDir.isEmpty) {
      throw StateError(
        'The apps/browser-service directory could not be located.',
      );
    }
    await _rpc.request('browser.startService', params: {'dir': _serviceDir});
    _serviceRunning = true;
    final launched = await _call('browser.launch');
    _sessionSequence++;
    final profile = launched['profile']?.toString();
    final session = BrowserSession(
      id: 'core-browser-$_sessionSequence',
      profileId: profile == null || profile.isEmpty
          ? 'ephemeral-profile-$_sessionSequence'
          : _profileLabel(profile),
      status: BrowserRuntimeStatus.running,
      tabs: const [
        BrowserTab(id: 'core-tab', title: 'New tab', url: 'about:blank'),
      ],
      activeTabId: 'core-tab',
      viewport: const BrowserViewport(width: 1280, height: 720),
    );
    return _emit(BrowserSnapshot(session: session));
  }

  @override
  Future<BrowserSnapshot> close() async {
    if (_serviceRunning) {
      try {
        await _call('browser.close');
      } finally {
        await _rpc.request('browser.stopService');
      }
    }
    _serviceRunning = false;
    return _emit(_snapshot.copyWith(clearSession: true));
  }

  @override
  Future<BrowserSnapshot> recover() async {
    final previous = _snapshot;
    if (_serviceRunning) {
      try {
        await _rpc.request('browser.stopService');
      } on Object {
        // The child may already have exited.
      }
      _serviceRunning = false;
    }
    final recoveryCount = (_snapshot.session?.recoveryCount ?? 0) + 1;
    final recovered = await launch();
    return _emit(
      recovered.copyWith(
        session: recovered.session?.copyWith(recoveryCount: recoveryCount),
        evidence: previous.evidence,
        artifacts: previous.artifacts,
        screenshotPath: previous.screenshotPath,
      ),
    );
  }

  @override
  Future<BrowserSnapshot> navigate(
    String url, {
    Map<String, dynamic> metadata = const {},
  }) async {
    if (_snapshot.session == null) await launch();
    final parsed = Uri.tryParse(url.trim());
    if (parsed == null ||
        (parsed.scheme != 'http' && parsed.scheme != 'https') ||
        parsed.host.isEmpty) {
      throw ArgumentError('Only absolute HTTP(S) URLs are allowed.');
    }
    final result = await _call('browser.navigate', {'url': parsed.toString()});
    final session = _snapshot.session!;
    final tab = session.activeTab!.copyWith(
      url: maskBrowserText(result['url']?.toString() ?? parsed.toString()),
      title: result['title']?.toString() ?? parsed.host,
      status: BrowserTabStatus.ready,
    );
    final changed = _snapshot.copyWith(
      session: session.copyWith(
        tabs: [tab],
        previewMetadata: maskBrowserMap(metadata),
      ),
    );
    _snapshot = changed;
    return refreshEvidence();
  }

  @override
  Future<BrowserSnapshot> captureScreenshot({bool fullPage = false}) async {
    _requireSession();
    _artifactSequence++;
    final separator = Platform.pathSeparator;
    final path =
        '${Directory.systemTemp.path}${separator}retcon-browser-$_artifactSequence.png';
    final result = await _call('browser.screenshot', {
      'path': path,
      'fullPage': fullPage,
      'type': 'png',
    });
    final safePath = result['path']?.toString() ?? path;
    final artifact = BrowserArtifact(
      id: 'core-browser-artifact-$_artifactSequence',
      label: fullPage ? 'Full-page screenshot' : 'Viewport screenshot',
      path: safePath,
      createdAt: DateTime.now(),
      metadata: {'fullPage': fullPage},
    );
    return _emit(
      _snapshot.copyWith(
        screenshotPath: safePath,
        artifacts: [..._snapshot.artifacts, artifact],
        evidence: _bounded([
          ..._snapshot.evidence,
          BrowserEvidenceEntry(
            kind: BrowserEvidenceKind.screenshots,
            summary: artifact.label,
            createdAt: artifact.createdAt,
            details: {'path': safePath},
          ),
        ]),
      ),
    );
  }

  @override
  Future<BrowserSnapshot> refreshEvidence() async {
    _requireSession();
    final result = await _call('browser.logs', {
      'offset': 0,
      'limit': maxEvidenceEntries,
    });
    final entries = <BrowserEvidenceEntry>[];
    for (final item in _maps(result['console'])) {
      entries.add(
        BrowserEvidenceEntry(
          kind: item['type'] == 'error'
              ? BrowserEvidenceKind.errors
              : BrowserEvidenceKind.console,
          summary: maskBrowserText(item['text']?.toString() ?? item.toString()),
          createdAt: _date(item['timestamp']) ?? DateTime.now(),
          level: item['type']?.toString() ?? 'info',
          details: maskBrowserMap(item),
        ),
      );
    }
    for (final item in _maps(result['network'])) {
      entries.add(
        BrowserEvidenceEntry(
          kind: BrowserEvidenceKind.network,
          summary: maskBrowserText(
            '${item['method'] ?? item['status'] ?? ''} ${item['url'] ?? ''}',
          ).trim(),
          createdAt: DateTime.now(),
          details: maskBrowserMap(item),
        ),
      );
    }
    return _emit(_snapshot.copyWith(evidence: _bounded(entries)));
  }

  @override
  Future<BrowserSnapshot> performAction(BrowserAutomationAction action) async {
    _requireSession();
    final selector = action.selector.trim();
    if (selector.isEmpty || selector.length > 500) {
      throw ArgumentError('Use a non-empty selector under 500 characters.');
    }
    final actionName = switch (action.kind) {
      BrowserActionKind.readText => 'text',
      _ => action.kind.name,
    };
    final result = await _call('browser.action', {
      'action': actionName,
      'selector': selector,
      if (action.value != null) 'value': action.value,
    });
    final summary = action.kind == BrowserActionKind.fill
        ? 'Filled $selector with a masked value'
        : '${action.kind.name} completed on $selector';
    return _emit(
      _snapshot.copyWith(
        evidence: _bounded([
          ..._snapshot.evidence,
          BrowserEvidenceEntry(
            kind: BrowserEvidenceKind.artifacts,
            summary: summary,
            createdAt: DateTime.now(),
            details: maskBrowserMap({
              'action': actionName,
              'selector': selector,
              if (action.kind == BrowserActionKind.fill) 'value': maskedValue,
              if (action.kind != BrowserActionKind.fill) 'result': result,
            }),
          ),
        ]),
      ),
    );
  }

  @override
  Future<BrowserSnapshot> newTab({String url = 'about:blank'}) =>
      _unsupported('Multiple tabs');

  @override
  Future<BrowserSnapshot> closeTab(String tabId) =>
      _unsupported('Multiple tabs');

  @override
  Future<BrowserSnapshot> selectTab(String tabId) async => _snapshot;

  @override
  Future<BrowserSnapshot> back() => _unsupported('History navigation');

  @override
  Future<BrowserSnapshot> forward() => _unsupported('History navigation');

  @override
  Future<BrowserSnapshot> reload() => _unsupported('Reload');

  @override
  Future<BrowserSnapshot> stopLoading() => _unsupported('Stop loading');

  @override
  Future<BrowserSnapshot> setViewport(BrowserViewport viewport) =>
      _unsupported('Viewport emulation');

  @override
  Future<BrowserSnapshot> pauseAutomation({required String reason}) =>
      _unsupported('Manual takeover');

  @override
  Future<BrowserSnapshot> openHeadedTakeover() =>
      _unsupported('Headed takeover');

  @override
  Future<BrowserSnapshot> resumeAutomation() => _unsupported('Manual takeover');

  Future<BrowserSnapshot> _refreshStatus() async {
    final status = await _call('browser.status');
    final session = _snapshot.session;
    if (session == null) return _snapshot;
    final url = status['url']?.toString();
    final active = session.activeTab;
    final updated = active == null || url == null
        ? session
        : session.copyWith(tabs: [active.copyWith(url: maskBrowserText(url))]);
    return _emit(_snapshot.copyWith(session: updated));
  }

  Future<Map<String, dynamic>> _call(
    String method, [
    Map<String, dynamic> params = const {},
  ]) => _rpc.request(
    'browser.call',
    params: {'method': method, 'params': params},
  );

  void _onEvent(Map<String, dynamic> wire) {
    final envelope = _map(wire['event']).isEmpty ? wire : _map(wire['event']);
    final kind = envelope['kind']?.toString();
    if (kind == 'browser.serviceExited') {
      _serviceRunning = false;
      final session = _snapshot.session;
      if (session != null) {
        _emit(
          _snapshot.copyWith(
            session: session.copyWith(
              status: BrowserRuntimeStatus.crashed,
              automationPaused: true,
              crashMessage: 'The browser service exited unexpectedly.',
            ),
          ),
        );
      }
      return;
    }
    if (kind != 'browser.event') return;
    final outerPayload = _map(envelope['payload']);
    final inner = _map(outerPayload['event']);
    final type = inner['type']?.toString();
    final payload = _map(inner['payload']);
    if (type == null) return;
    if (type == 'browser.crashed') {
      final session = _snapshot.session;
      if (session != null) {
        _emit(
          _snapshot.copyWith(
            session: session.copyWith(
              status: BrowserRuntimeStatus.crashed,
              automationPaused: true,
              crashMessage: 'The managed browser context crashed.',
            ),
          ),
        );
      }
      return;
    }
    final evidenceKind = switch (type) {
      'browser.console' when payload['type'] == 'error' =>
        BrowserEvidenceKind.errors,
      'browser.console' => BrowserEvidenceKind.console,
      'browser.request' || 'browser.response' => BrowserEvidenceKind.network,
      _ => BrowserEvidenceKind.artifacts,
    };
    _emit(
      _snapshot.copyWith(
        evidence: _bounded([
          ..._snapshot.evidence,
          BrowserEvidenceEntry(
            kind: evidenceKind,
            summary: maskBrowserText(
              payload['text']?.toString() ?? payload['url']?.toString() ?? type,
            ),
            createdAt: _date(payload['timestamp']) ?? DateTime.now(),
            level: payload['type']?.toString() ?? 'info',
            details: maskBrowserMap(payload),
          ),
        ]),
      ),
    );
  }

  BrowserSession _requireSession() {
    final session = _snapshot.session;
    if (session == null) throw StateError('Start a browser session first.');
    if (session.status == BrowserRuntimeStatus.crashed) {
      throw StateError('Recover the browser session first.');
    }
    return session;
  }

  Future<BrowserSnapshot> _unsupported(String feature) =>
      Future.error(UnsupportedError('$feature is not exposed by Core yet.'));

  BrowserSnapshot _emit(BrowserSnapshot snapshot) {
    _snapshot = snapshot;
    _events.add(snapshot);
    return snapshot;
  }

  Future<void> dispose() async {
    await _subscription?.cancel();
    await _events.close();
  }
}

Map<String, dynamic> _map(Object? value) =>
    value is Map ? value.cast<String, dynamic>() : <String, dynamic>{};

List<Map<String, dynamic>> _maps(Object? value) => (value as List? ?? const [])
    .whereType<Map>()
    .map((item) => item.cast<String, dynamic>())
    .toList();

List<BrowserEvidenceEntry> _bounded(List<BrowserEvidenceEntry> entries) =>
    entries.length <= CoreBrowserRepository.maxEvidenceEntries
    ? entries
    : entries.sublist(
        entries.length - CoreBrowserRepository.maxEvidenceEntries,
      );

DateTime? _date(Object? value) => value is String
    ? DateTime.tryParse(value)
    : value is num
    ? DateTime.fromMillisecondsSinceEpoch(value.toInt())
    : null;

String _profileLabel(String path) {
  final parts = path.split(RegExp(r'[/\\]'));
  return parts.isEmpty ? 'ephemeral-profile' : parts.last;
}
