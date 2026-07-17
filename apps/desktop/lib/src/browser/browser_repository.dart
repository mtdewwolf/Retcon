import 'dart:async';

import 'browser_models.dart';

abstract interface class BrowserRepository {
  BrowserCapabilities get capabilities;
  Stream<BrowserSnapshot> get events;

  Future<BrowserSnapshot> load();
  Future<BrowserSnapshot> launch();
  Future<BrowserSnapshot> close();
  Future<BrowserSnapshot> recover();
  Future<BrowserSnapshot> newTab({String url = 'about:blank'});
  Future<BrowserSnapshot> closeTab(String tabId);
  Future<BrowserSnapshot> selectTab(String tabId);
  Future<BrowserSnapshot> navigate(String url, {Map<String, dynamic> metadata});
  Future<BrowserSnapshot> back();
  Future<BrowserSnapshot> forward();
  Future<BrowserSnapshot> reload();
  Future<BrowserSnapshot> stopLoading();
  Future<BrowserSnapshot> setViewport(BrowserViewport viewport);
  Future<BrowserSnapshot> captureScreenshot({bool fullPage = false});
  Future<BrowserSnapshot> refreshEvidence();
  Future<BrowserSnapshot> performAction(BrowserAutomationAction action);
  Future<BrowserSnapshot> pauseAutomation({required String reason});
  Future<BrowserSnapshot> openHeadedTakeover();
  Future<BrowserSnapshot> resumeAutomation();
}

class InMemoryBrowserRepository implements BrowserRepository {
  InMemoryBrowserRepository({BrowserSnapshot? initial})
    : _snapshot = initial ?? const BrowserSnapshot();

  factory InMemoryBrowserRepository.demo() => InMemoryBrowserRepository();

  static const maxEvidenceEntries = 200;
  final _events = StreamController<BrowserSnapshot>.broadcast(sync: true);
  final Map<String, List<String>> _history = {};
  final Map<String, int> _historyIndex = {};
  BrowserSnapshot _snapshot;
  int _sessionSequence = 0;
  int _tabSequence = 0;
  int _artifactSequence = 0;

  @override
  BrowserCapabilities get capabilities => const BrowserCapabilities();

  @override
  Stream<BrowserSnapshot> get events => _events.stream;

  @override
  Future<BrowserSnapshot> load() async => _snapshot;

  @override
  Future<BrowserSnapshot> launch() async {
    final existing = _snapshot.session;
    if (existing != null &&
        existing.status != BrowserRuntimeStatus.stopped &&
        existing.status != BrowserRuntimeStatus.crashed) {
      return _snapshot;
    }
    _sessionSequence++;
    final tab = _makeTab('about:blank');
    _history[tab.id] = [tab.url];
    _historyIndex[tab.id] = 0;
    return _emit(
      BrowserSnapshot(
        session: BrowserSession(
          id: 'browser-session-$_sessionSequence',
          profileId: 'isolated-profile-$_sessionSequence',
          status: BrowserRuntimeStatus.running,
          tabs: [tab],
          activeTabId: tab.id,
          viewport: const BrowserViewport(width: 1280, height: 720),
        ),
      ),
    );
  }

  @override
  Future<BrowserSnapshot> close() async {
    _history.clear();
    _historyIndex.clear();
    return _emit(_snapshot.copyWith(clearSession: true));
  }

  @override
  Future<BrowserSnapshot> recover() async {
    final crashed = _snapshot.session;
    if (crashed == null || crashed.status != BrowserRuntimeStatus.crashed) {
      return launch();
    }
    final tab = _makeTab(crashed.activeTab?.url ?? 'about:blank');
    _history[tab.id] = [tab.url];
    _historyIndex[tab.id] = 0;
    return _emit(
      _snapshot.copyWith(
        session: BrowserSession(
          id: crashed.id,
          profileId: crashed.profileId,
          status: BrowserRuntimeStatus.running,
          tabs: [tab],
          activeTabId: tab.id,
          viewport: crashed.viewport,
          recoveryCount: crashed.recoveryCount + 1,
          previewMetadata: crashed.previewMetadata,
          takeoverHistory: crashed.takeoverHistory,
        ),
      ),
    );
  }

  @override
  Future<BrowserSnapshot> newTab({String url = 'about:blank'}) async {
    final session = _runningSession();
    final tab = _makeTab(_safeUrl(url, allowBlank: true));
    _history[tab.id] = [tab.url];
    _historyIndex[tab.id] = 0;
    return _emit(
      _snapshot.copyWith(
        session: session.copyWith(
          tabs: [...session.tabs, tab],
          activeTabId: tab.id,
        ),
      ),
    );
  }

  @override
  Future<BrowserSnapshot> closeTab(String tabId) async {
    final session = _runningSession();
    if (session.tabs.length == 1) return close();
    final tabs = session.tabs.where((tab) => tab.id != tabId).toList();
    _history.remove(tabId);
    _historyIndex.remove(tabId);
    final active = session.activeTabId == tabId
        ? tabs.first.id
        : session.activeTabId;
    return _emit(
      _snapshot.copyWith(
        session: session.copyWith(tabs: tabs, activeTabId: active),
      ),
    );
  }

  @override
  Future<BrowserSnapshot> selectTab(String tabId) async {
    final session = _runningSession();
    if (!session.tabs.any((tab) => tab.id == tabId)) return _snapshot;
    return _emit(
      _snapshot.copyWith(session: session.copyWith(activeTabId: tabId)),
    );
  }

  @override
  Future<BrowserSnapshot> navigate(
    String url, {
    Map<String, dynamic> metadata = const {},
  }) async {
    final session = _runningSession(allowPaused: false);
    final safeUrl = _safeUrl(url);
    final tab = session.activeTab!;
    final history = _history.putIfAbsent(tab.id, () => [tab.url]);
    final currentIndex = _historyIndex[tab.id] ?? history.length - 1;
    if (currentIndex < history.length - 1) {
      history.removeRange(currentIndex + 1, history.length);
    }
    history.add(safeUrl);
    _historyIndex[tab.id] = history.length - 1;
    final updated = tab.copyWith(
      title: _title(safeUrl),
      url: maskBrowserText(safeUrl),
      status: BrowserTabStatus.ready,
      canGoBack: history.length > 1,
      canGoForward: false,
    );
    final evidence = [
      ..._snapshot.evidence,
      BrowserEvidenceEntry(
        kind: BrowserEvidenceKind.network,
        summary: 'GET ${maskBrowserText(safeUrl)} · 200',
        createdAt: DateTime.now(),
        details: maskBrowserMap({
          'method': 'GET',
          'url': safeUrl,
          'headers': {'authorization': 'Bearer private'},
        }),
      ),
      BrowserEvidenceEntry(
        kind: BrowserEvidenceKind.console,
        summary: 'Preview ready',
        createdAt: DateTime.now(),
      ),
      BrowserEvidenceEntry(
        kind: BrowserEvidenceKind.accessibility,
        summary: 'Document · ${_title(safeUrl)}',
        createdAt: DateTime.now(),
        details: const {'role': 'document', 'children': 1},
      ),
      BrowserEvidenceEntry(
        kind: BrowserEvidenceKind.performance,
        summary: 'DOMContentLoaded 184 ms',
        createdAt: DateTime.now(),
        details: const {'domContentLoadedMs': 184, 'firstPaintMs': 132},
      ),
    ];
    return _emit(
      _snapshot.copyWith(
        session: session.copyWith(
          tabs: _replaceTab(session.tabs, updated),
          previewMetadata: maskBrowserMap(metadata),
        ),
        evidence: _bound(evidence),
      ),
    );
  }

  @override
  Future<BrowserSnapshot> back() => _moveHistory(-1);

  @override
  Future<BrowserSnapshot> forward() => _moveHistory(1);

  Future<BrowserSnapshot> _moveHistory(int delta) async {
    final session = _runningSession(allowPaused: false);
    final tab = session.activeTab!;
    final history = _history[tab.id] ?? [tab.url];
    final current = _historyIndex[tab.id] ?? history.length - 1;
    final next = (current + delta).clamp(0, history.length - 1);
    _historyIndex[tab.id] = next;
    final updated = tab.copyWith(
      url: maskBrowserText(history[next]),
      title: _title(history[next]),
      canGoBack: next > 0,
      canGoForward: next < history.length - 1,
    );
    return _emit(
      _snapshot.copyWith(
        session: session.copyWith(tabs: _replaceTab(session.tabs, updated)),
      ),
    );
  }

  @override
  Future<BrowserSnapshot> reload() async {
    final session = _runningSession(allowPaused: false);
    return _addEvidence(
      BrowserEvidenceEntry(
        kind: BrowserEvidenceKind.network,
        summary: 'Reloaded ${maskBrowserText(session.activeTab!.url)}',
        createdAt: DateTime.now(),
      ),
    );
  }

  @override
  Future<BrowserSnapshot> stopLoading() async {
    final session = _runningSession(allowPaused: false);
    final tab = session.activeTab!;
    return _emit(
      _snapshot.copyWith(
        session: session.copyWith(
          tabs: _replaceTab(
            session.tabs,
            tab.copyWith(status: BrowserTabStatus.ready),
          ),
        ),
      ),
    );
  }

  @override
  Future<BrowserSnapshot> setViewport(BrowserViewport viewport) async {
    final session = _runningSession();
    if (viewport.width < 320 || viewport.height < 240) {
      throw ArgumentError('Viewport must be at least 320 × 240.');
    }
    return _emit(
      _snapshot.copyWith(session: session.copyWith(viewport: viewport)),
    );
  }

  @override
  Future<BrowserSnapshot> captureScreenshot({bool fullPage = false}) async {
    _runningSession();
    _artifactSequence++;
    final path = 'artifacts/browser/screenshot-$_artifactSequence.png';
    final artifact = BrowserArtifact(
      id: 'browser-artifact-$_artifactSequence',
      label: fullPage ? 'Full-page screenshot' : 'Viewport screenshot',
      path: path,
      createdAt: DateTime.now(),
      metadata: {'fullPage': fullPage},
    );
    return _emit(
      _snapshot.copyWith(
        screenshotPath: path,
        artifacts: [..._snapshot.artifacts, artifact],
        evidence: _bound([
          ..._snapshot.evidence,
          BrowserEvidenceEntry(
            kind: BrowserEvidenceKind.screenshots,
            summary: artifact.label,
            createdAt: artifact.createdAt,
            details: {'path': path},
          ),
        ]),
      ),
    );
  }

  @override
  Future<BrowserSnapshot> refreshEvidence() async => _snapshot;

  @override
  Future<BrowserSnapshot> performAction(BrowserAutomationAction action) async {
    _runningSession(allowPaused: false);
    final selector = action.selector.trim();
    if (selector.isEmpty || selector.length > 500) {
      throw ArgumentError('Use a non-empty selector under 500 characters.');
    }
    final summary = switch (action.kind) {
      BrowserActionKind.click => 'Clicked $selector',
      BrowserActionKind.fill => 'Filled $selector with a masked value',
      BrowserActionKind.press =>
        'Pressed ${maskBrowserText(action.value ?? 'Enter')} on $selector',
      BrowserActionKind.readText => 'Read text from $selector',
    };
    return _addEvidence(
      BrowserEvidenceEntry(
        kind: BrowserEvidenceKind.artifacts,
        summary: summary,
        createdAt: DateTime.now(),
        details: {
          'action': action.kind.name,
          'selector': selector,
          if (action.kind == BrowserActionKind.fill) 'value': maskedValue,
        },
      ),
    );
  }

  @override
  Future<BrowserSnapshot> pauseAutomation({required String reason}) async {
    final session = _runningSession();
    final interval = BrowserTakeoverInterval(
      startedAt: DateTime.now(),
      reason: reason.trim().isEmpty ? 'Manual inspection' : reason.trim(),
    );
    return _emit(
      _snapshot.copyWith(
        session: session.copyWith(
          status: BrowserRuntimeStatus.paused,
          automationPaused: true,
          takeoverHistory: [...session.takeoverHistory, interval],
        ),
      ),
    );
  }

  @override
  Future<BrowserSnapshot> openHeadedTakeover() async {
    final session = _runningSession();
    final history = [...session.takeoverHistory];
    if (history.isEmpty || history.last.endedAt != null) {
      history.add(
        BrowserTakeoverInterval(
          startedAt: DateTime.now(),
          reason: 'Manual headed takeover',
          openedHeaded: true,
        ),
      );
    } else {
      history[history.length - 1] = history.last.copyWith(openedHeaded: true);
    }
    return _emit(
      _snapshot.copyWith(
        session: session.copyWith(
          status: BrowserRuntimeStatus.paused,
          automationPaused: true,
          headless: false,
          takeoverHistory: history,
        ),
      ),
    );
  }

  @override
  Future<BrowserSnapshot> resumeAutomation() async {
    final session = _runningSession();
    final history = [...session.takeoverHistory];
    if (history.isNotEmpty && history.last.endedAt == null) {
      history[history.length - 1] = history.last.copyWith(
        endedAt: DateTime.now(),
      );
    }
    return _emit(
      _snapshot.copyWith(
        session: session.copyWith(
          status: BrowserRuntimeStatus.running,
          automationPaused: false,
          headless: true,
          takeoverHistory: history,
        ),
      ),
    );
  }

  BrowserSnapshot simulateCrash({String message = 'Browser process exited.'}) {
    final session = _snapshot.session;
    if (session == null) throw StateError('No browser session is running.');
    return _emit(
      _snapshot.copyWith(
        session: session.copyWith(
          status: BrowserRuntimeStatus.crashed,
          crashMessage: message,
          automationPaused: true,
        ),
      ),
    );
  }

  BrowserSnapshot _addEvidence(BrowserEvidenceEntry entry) => _emit(
    _snapshot.copyWith(evidence: _bound([..._snapshot.evidence, entry])),
  );

  BrowserSession _runningSession({bool allowPaused = true}) {
    final session = _snapshot.session;
    if (session == null || session.status == BrowserRuntimeStatus.stopped) {
      throw StateError('Start a managed browser session first.');
    }
    if (session.status == BrowserRuntimeStatus.crashed) {
      throw StateError('Recover the crashed browser session first.');
    }
    if (!allowPaused && session.automationPaused) {
      throw StateError('Resume automation before controlling the page.');
    }
    return session;
  }

  BrowserTab _makeTab(String url) {
    _tabSequence++;
    return BrowserTab(id: 'tab-$_tabSequence', title: _title(url), url: url);
  }

  BrowserSnapshot _emit(BrowserSnapshot snapshot) {
    _snapshot = snapshot;
    _events.add(snapshot);
    return snapshot;
  }
}

List<BrowserTab> _replaceTab(List<BrowserTab> tabs, BrowserTab updated) => [
  for (final tab in tabs)
    if (tab.id == updated.id) updated else tab,
];

List<BrowserEvidenceEntry> _bound(List<BrowserEvidenceEntry> entries) =>
    entries.length <= InMemoryBrowserRepository.maxEvidenceEntries
    ? entries
    : entries.sublist(
        entries.length - InMemoryBrowserRepository.maxEvidenceEntries,
      );

String _safeUrl(String input, {bool allowBlank = false}) {
  final trimmed = input.trim();
  if (allowBlank && trimmed == 'about:blank') return trimmed;
  final parsed = Uri.tryParse(trimmed);
  if (parsed == null ||
      (parsed.scheme != 'http' && parsed.scheme != 'https') ||
      parsed.host.isEmpty) {
    throw ArgumentError('Only absolute HTTP(S) URLs are allowed.');
  }
  return parsed.toString();
}

String _title(String url) {
  if (url == 'about:blank') return 'New tab';
  return Uri.tryParse(url)?.host ?? 'Page';
}

const maskedValue = '••••••••';

bool _sensitiveKey(String key) {
  final lower = key.toLowerCase();
  return lower.contains('cookie') ||
      lower.contains('authorization') ||
      lower.contains('token') ||
      lower.contains('secret') ||
      lower.contains('password') ||
      lower.contains('api_key') ||
      lower.contains('apikey');
}

Map<String, dynamic> maskBrowserMap(Map<String, dynamic> value) => {
  for (final entry in value.entries)
    entry.key: _sensitiveKey(entry.key)
        ? maskedValue
        : switch (entry.value) {
            Map map => maskBrowserMap(map.cast<String, dynamic>()),
            List list => [
              for (final item in list)
                if (item is Map)
                  maskBrowserMap(item.cast<String, dynamic>())
                else if (item is String)
                  maskBrowserText(item)
                else
                  item,
            ],
            String item => maskBrowserText(item),
            final item => item,
          },
};

String maskBrowserText(String value) {
  final uri = Uri.tryParse(value);
  if (uri != null && uri.hasQuery && uri.scheme.isNotEmpty) {
    final query = {
      for (final entry in uri.queryParametersAll.entries)
        entry.key: _sensitiveKey(entry.key) ? [maskedValue] : entry.value,
    };
    return uri.replace(queryParameters: query).toString();
  }
  return value
      .replaceAll(
        RegExp(
          r'(authorization|cookie|token|secret|password|api[_-]?key)\s*[:=]\s*(?:bearer\s+)?[^\s,;]+',
          caseSensitive: false,
        ),
        maskedValue,
      )
      .replaceAll(
        RegExp(r'bearer\s+[a-z0-9._~+/-]+', caseSensitive: false),
        'Bearer $maskedValue',
      );
}
