import 'dart:async';
import 'dart:convert';
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

  Future<String?> readArtifact(String hash, {required int maxBytes});
  String? artifactPath(String hash);
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

  @override
  Future<String?> readArtifact(String hash, {required int maxBytes}) async {
    final path = artifactPath(hash);
    if (path == null || maxBytes <= 0) return null;
    try {
      final handle = await File(path).open();
      try {
        final length = await handle.length();
        final bytes = await handle.read(maxBytes);
        final text = utf8.decode(bytes, allowMalformed: true);
        return length > bytes.length ? '$text\n… [artifact truncated]' : text;
      } finally {
        await handle.close();
      }
    } on FileSystemException {
      return null;
    }
  }

  @override
  String? artifactPath(String hash) {
    if (!_artifactHash.hasMatch(hash)) return null;
    final separator = Platform.pathSeparator;
    return '${_core.dataDirectory.path}${separator}artifacts${separator}sha256'
        '$separator${hash.substring(0, 2)}$separator${hash.substring(2)}';
  }
}

class CoreBrowserRepository implements BrowserRepository {
  CoreBrowserRepository(
    this._rpc, {
    required this.projectId,
    this.taskId,
    this.maxArtifactBytes = 64 * 1024,
  }) {
    _subscription = _rpc.events.where(_isRefreshEvent).listen(_onEvent);
  }

  factory CoreBrowserRepository.fromCore(
    CoreClient core, {
    required String projectId,
    String? taskId,
  }) => CoreBrowserRepository(
    CoreBrowserRpcClient(core),
    projectId: projectId,
    taskId: taskId,
  );

  static const maxEvidenceEntries = 1000;
  final BrowserRpcClient _rpc;
  final String projectId;
  final String? taskId;
  final int maxArtifactBytes;
  final _events = StreamController<BrowserSnapshot>.broadcast(sync: true);
  StreamSubscription<Map<String, dynamic>>? _subscription;
  BrowserSnapshot _snapshot = const BrowserSnapshot();
  String? _sessionId;
  String? _activeTabId;
  String? _devServerInstanceId;
  Map<String, dynamic> _previewMetadata = const {};
  bool _openedHeaded = false;
  int _recoveryCount = 0;
  final Map<String, bool> _canGoBack = {};
  final Map<String, bool> _canGoForward = {};

  @override
  BrowserCapabilities get capabilities => const BrowserCapabilities(
    multipleTabs: true,
    historyNavigation: true,
    reload: true,
    stopLoading: false,
    viewportAndDevice: false,
    screenshots: true,
    consoleAndNetwork: true,
    accessibility: true,
    performance: false,
    automation: true,
    headedTakeover: true,
    cookiesAndStorage: false,
  );

  @override
  Stream<BrowserSnapshot> get events => _events.stream;

  @override
  Future<BrowserSnapshot> load() async {
    final response = await _rpc.request(
      'browser.session.list',
      params: {'projectId': projectId},
    );
    final sessions = _maps(response['sessions']);
    final active = sessions.where((session) {
      final status = session['status']?.toString();
      return status == 'running' ||
          status == 'starting' ||
          status == 'orphaned';
    }).firstOrNull;
    if (active == null) return _emit(const BrowserSnapshot());
    _sessionId = active['id']?.toString();
    _devServerInstanceId = active['devServerInstanceId']?.toString();
    return _refreshSession();
  }

  @override
  Future<BrowserSnapshot> launch({
    String? taskId,
    String? devServerInstanceId,
  }) async {
    final response = await _rpc.request(
      'browser.session.start',
      params: {
        'projectId': projectId,
        if (_validUuid(taskId ?? this.taskId)) 'taskId': taskId ?? this.taskId,
        if (_validUuid(devServerInstanceId))
          'devServerInstanceId': devServerInstanceId,
        'persistentProfile': false,
        'networkPolicy': 'loopback',
      },
    );
    final session = _map(response['session']);
    _sessionId = session['id']?.toString();
    _devServerInstanceId = session['devServerInstanceId']?.toString();
    final initial = _map(response['initialTab']);
    if (initial.isNotEmpty) _activeTabId = initial['id']?.toString();
    return _refreshSession();
  }

  @override
  Future<BrowserSnapshot> close() async {
    final sessionId = _requireSessionId();
    await _rpc.request(
      'browser.session.stop',
      params: {'sessionId': sessionId},
    );
    _sessionId = null;
    _activeTabId = null;
    _openedHeaded = false;
    return _emit(_snapshot.copyWith(clearSession: true));
  }

  @override
  Future<BrowserSnapshot> recover() async {
    final previous = _snapshot;
    final binding = _devServerInstanceId;
    _recoveryCount++;
    _sessionId = null;
    final recovered = await launch(
      taskId: taskId,
      devServerInstanceId: binding,
    );
    return _emit(
      recovered.copyWith(
        session: recovered.session?.copyWith(recoveryCount: _recoveryCount),
        evidence: previous.evidence,
        artifacts: previous.artifacts,
        screenshotPath: previous.screenshotPath,
      ),
    );
  }

  @override
  Future<BrowserSnapshot> newTab({String url = 'about:blank'}) async {
    final response = await _operation('browser.tab.open', {'url': url});
    final tab = _map(response['tab']);
    if (tab.isNotEmpty) _activeTabId = tab['id']?.toString();
    return _refreshSession();
  }

  @override
  Future<BrowserSnapshot> closeTab(String tabId) async {
    await _operation('browser.tab.close', {'tabId': tabId});
    if (_activeTabId == tabId) _activeTabId = null;
    return _refreshSession();
  }

  @override
  Future<BrowserSnapshot> selectTab(String tabId) async {
    await _operation('browser.tab.activate', {'tabId': tabId});
    _activeTabId = tabId;
    return _refreshSession();
  }

  @override
  Future<BrowserSnapshot> navigate(
    String url, {
    Map<String, dynamic> metadata = const {},
  }) async {
    if (_sessionId == null) {
      final binding = metadata['devServerInstanceId']?.toString();
      await launch(devServerInstanceId: binding);
    }
    if (_activeTabId == null) await newTab();
    final tabId = _requireTabId();
    await _operation('browser.navigate', {'tabId': tabId, 'url': url});
    _canGoBack[tabId] = true;
    _canGoForward[tabId] = false;
    _previewMetadata = maskBrowserMap(metadata);
    return _refreshSession();
  }

  @override
  Future<BrowserSnapshot> back() async {
    final tabId = _requireTabId();
    await _operation('browser.back', {'tabId': tabId});
    _canGoForward[tabId] = true;
    return _refreshSession();
  }

  @override
  Future<BrowserSnapshot> forward() async {
    final tabId = _requireTabId();
    await _operation('browser.forward', {'tabId': tabId});
    return _refreshSession();
  }

  @override
  Future<BrowserSnapshot> reload() async {
    await _operation('browser.reload', {'tabId': _requireTabId()});
    return _refreshSession();
  }

  @override
  Future<BrowserSnapshot> stopLoading() => Future.error(
    UnsupportedError('The durable Core contract does not expose stop loading.'),
  );

  @override
  Future<BrowserSnapshot> setViewport(BrowserViewport viewport) => Future.error(
    UnsupportedError(
      'The durable Core contract does not expose device emulation.',
    ),
  );

  @override
  Future<BrowserSnapshot> captureScreenshot({bool fullPage = false}) async {
    final response = await _operation('browser.observation.screenshot', {
      'tabId': _requireTabId(),
      'fullPage': fullPage,
      'type': 'png',
    });
    await _mergeOperationArtifacts(response);
    return _refreshObservations();
  }

  @override
  Future<BrowserSnapshot> refreshEvidence() async {
    final tabId = _activeTabId;
    try {
      await _operation('browser.observation.logs', {'tabId': ?tabId});
    } on Object {
      // Durable observations already recorded remain available.
    }
    try {
      await _operation('browser.observation.snapshot', {'tabId': ?tabId});
    } on Object {
      // Accessibility snapshots depend on the installed service feature set.
    }
    return _refreshObservations();
  }

  @override
  Future<BrowserSnapshot> performAction(BrowserAutomationAction action) async {
    final actionName = switch (action.kind) {
      BrowserActionKind.readText => 'text',
      _ => action.kind.name,
    };
    await _operation('browser.automation.action', {
      'tabId': _requireTabId(),
      'action': actionName,
      'selector': action.selector,
      if (action.value != null) 'value': action.value,
    });
    return _emit(
      _snapshot.copyWith(
        evidence: _bounded([
          ..._snapshot.evidence,
          BrowserEvidenceEntry(
            kind: BrowserEvidenceKind.artifacts,
            summary: action.kind == BrowserActionKind.fill
                ? 'Filled ${action.selector} with a masked value'
                : '${action.kind.name} completed on ${action.selector}',
            createdAt: DateTime.now(),
            details: {
              'action': actionName,
              'selector': action.selector,
              if (action.kind == BrowserActionKind.fill) 'value': maskedValue,
            },
          ),
        ]),
      ),
    );
  }

  @override
  Future<BrowserSnapshot> pauseAutomation({required String reason}) async {
    if (_snapshot.session?.automationPaused != true) {
      await _rpc.request(
        'browser.takeover.start',
        params: {'sessionId': _requireSessionId(), 'reason': reason},
      );
    }
    return _refreshSession();
  }

  @override
  Future<BrowserSnapshot> openHeadedTakeover() async {
    _openedHeaded = true;
    if (_snapshot.session?.automationPaused != true) {
      await _rpc.request(
        'browser.takeover.start',
        params: {
          'sessionId': _requireSessionId(),
          'reason': 'Manual headed takeover',
        },
      );
    }
    return _refreshSession();
  }

  @override
  Future<BrowserSnapshot> resumeAutomation() async {
    await _rpc.request(
      'browser.takeover.stop',
      params: {'sessionId': _requireSessionId()},
    );
    _openedHeaded = false;
    return _refreshSession();
  }

  Future<Map<String, dynamic>> _operation(
    String method,
    Map<String, dynamic> params,
  ) => _rpc.request(
    method,
    params: {'sessionId': _requireSessionId(), ...params},
  );

  Future<BrowserSnapshot> _refreshSession() async {
    final sessionId = _requireSessionId();
    final status = await _rpc.request(
      'browser.session.status',
      params: {'sessionId': sessionId},
    );
    final historyResponse = await _rpc.request(
      'browser.session.history',
      params: {'sessionId': sessionId},
    );
    final sessionWire = _map(status['session']);
    final tabsWire = _maps(status['tabs']);
    final takeover = _map(status['takeover']);
    final history = _maps(
      historyResponse['events'],
    ).map(_decodeHistory).toList();
    final tabs = tabsWire.where((tab) => tab['status'] != 'closed').map((tab) {
      final id = tab['id']?.toString() ?? '';
      return BrowserTab(
        id: id,
        title: tab['title']?.toString() ?? 'New tab',
        url: maskBrowserText(tab['url']?.toString() ?? 'about:blank'),
        status: tab['status'] == 'open'
            ? BrowserTabStatus.ready
            : BrowserTabStatus.failed,
        canGoBack: _canGoBack[id] == true,
        canGoForward: _canGoForward[id] == true,
      );
    }).toList();
    if (_activeTabId == null || !tabs.any((tab) => tab.id == _activeTabId)) {
      _activeTabId = tabs.firstOrNull?.id;
    }
    _devServerInstanceId = sessionWire['devServerInstanceId']?.toString();
    final statusValue = _runtimeStatus(
      sessionWire['status'],
      takeover.isNotEmpty,
    );
    final intervals = _decodeTakeovers(history, takeover);
    final session = BrowserSession(
      id: sessionId,
      profileId: sessionWire['profileId']?.toString() ?? 'isolated-profile',
      status: statusValue,
      tabs: tabs,
      activeTabId: _activeTabId ?? '',
      viewport:
          _snapshot.session?.viewport ??
          const BrowserViewport(width: 1280, height: 720),
      headless: takeover.isEmpty || !_openedHeaded,
      automationPaused: takeover.isNotEmpty,
      crashMessage: sessionWire['failure']?.toString(),
      recoveryCount: _recoveryCount,
      previewMetadata: _previewMetadata,
      takeoverHistory: intervals,
      history: history,
    );
    _snapshot = _snapshot.copyWith(session: session);
    return _refreshObservations();
  }

  Future<BrowserSnapshot> _refreshObservations() async {
    final response = await _rpc.request(
      'browser.observation.list',
      params: {'sessionId': _requireSessionId()},
    );
    final evidence = <BrowserEvidenceEntry>[];
    for (final item in _maps(response['console'])) {
      evidence.add(_logEvidence(item, console: true));
    }
    for (final item in _maps(response['network'])) {
      evidence.add(_logEvidence(item, console: false));
    }
    final artifacts = <BrowserArtifact>[];
    String? screenshotPath = _snapshot.screenshotPath;
    for (final item in _maps(response['observations'])) {
      final decoded = await _decodeObservation(item);
      artifacts.add(decoded.$1);
      evidence.add(decoded.$2);
      if (item['kind'] == 'screenshot') screenshotPath = decoded.$1.path;
    }
    return _emit(
      _snapshot.copyWith(
        evidence: _bounded(evidence),
        artifacts: artifacts,
        screenshotPath: screenshotPath,
      ),
    );
  }

  Future<void> _mergeOperationArtifacts(Map<String, dynamic> response) async {
    for (final observation in _maps(response['artifacts'])) {
      await _decodeObservation(observation);
    }
  }

  Future<(BrowserArtifact, BrowserEvidenceEntry)> _decodeObservation(
    Map<String, dynamic> item,
  ) async {
    final hash = item['artifactHash']?.toString() ?? '';
    final mime = item['mimeType']?.toString() ?? 'application/octet-stream';
    final kind = item['kind']?.toString() ?? 'artifact';
    final metadata = maskBrowserMap(_map(item['metadata']));
    if ((mime.startsWith('text/') || mime.contains('json')) &&
        _artifactHash.hasMatch(hash)) {
      final preview = await _rpc.readArtifact(hash, maxBytes: maxArtifactBytes);
      if (preview != null) {
        metadata['contentPreview'] = maskBrowserText(preview);
      }
    }
    final path = _rpc.artifactPath(hash) ?? 'artifact:$hash';
    final createdAt = _date(item['createdAt']) ?? DateTime.now();
    final artifact = BrowserArtifact(
      id: item['id']?.toString() ?? hash,
      label: _artifactLabel(kind),
      path: path,
      createdAt: createdAt,
      metadata: {
        ...metadata,
        'artifactHash': hash,
        'mimeType': mime,
        'sizeBytes': item['sizeBytes'],
      },
    );
    final evidenceKind = switch (kind) {
      'screenshot' => BrowserEvidenceKind.screenshots,
      'snapshot' || 'accessibility' => BrowserEvidenceKind.accessibility,
      'trace' => BrowserEvidenceKind.performance,
      'logs' => BrowserEvidenceKind.artifacts,
      _ => BrowserEvidenceKind.artifacts,
    };
    return (
      artifact,
      BrowserEvidenceEntry(
        kind: evidenceKind,
        summary: artifact.label,
        createdAt: createdAt,
        details: artifact.metadata,
      ),
    );
  }

  BrowserEvidenceEntry _logEvidence(
    Map<String, dynamic> item, {
    required bool console,
  }) {
    final isError = console && item['type']?.toString() == 'error';
    return BrowserEvidenceEntry(
      kind: isError
          ? BrowserEvidenceKind.errors
          : console
          ? BrowserEvidenceKind.console
          : BrowserEvidenceKind.network,
      summary: maskBrowserText(
        item['text']?.toString() ??
            '${item['method'] ?? item['status'] ?? ''} ${item['url'] ?? ''}',
      ).trim(),
      createdAt: _date(item['timestamp']) ?? DateTime.now(),
      level: item['type']?.toString() ?? 'info',
      details: maskBrowserMap(item),
    );
  }

  void _onEvent(Map<String, dynamic> wire) {
    final envelope = _envelope(wire);
    final payload = _map(envelope['payload']);
    if (payload['projectId']?.toString() != projectId) return;
    final eventSession = payload['sessionId']?.toString();
    if (_sessionId == null && eventSession != null) _sessionId = eventSession;
    if (eventSession != null && eventSession != _sessionId) return;
    unawaited(_refreshSession().catchError((Object _) => _snapshot));
  }

  String _requireSessionId() {
    final value = _sessionId;
    if (value == null || value.isEmpty) {
      throw StateError('Start a durable browser session first.');
    }
    return value;
  }

  String _requireTabId() {
    final value = _activeTabId;
    if (value == null || value.isEmpty) {
      throw StateError('Open a browser tab first.');
    }
    return value;
  }

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

BrowserHistoryEntry _decodeHistory(Map<String, dynamic> item) =>
    BrowserHistoryEntry(
      kind: item['kind']?.toString() ?? 'updated',
      actor: item['actor']?.toString() ?? 'system',
      createdAt: _date(item['createdAt']) ?? DateTime.now(),
      details: maskBrowserMap(_map(item['payload'])),
    );

List<BrowserTakeoverInterval> _decodeTakeovers(
  List<BrowserHistoryEntry> history,
  Map<String, dynamic> active,
) {
  final intervals = <BrowserTakeoverInterval>[];
  for (final event in history) {
    if (event.kind == 'takeover_started') {
      intervals.add(
        BrowserTakeoverInterval(
          startedAt: event.createdAt,
          reason: event.details['reason']?.toString() ?? 'Manual inspection',
          openedHeaded: true,
        ),
      );
    } else if (event.kind == 'takeover_stopped' && intervals.isNotEmpty) {
      intervals[intervals.length - 1] = intervals.last.copyWith(
        endedAt: event.createdAt,
      );
    }
  }
  if (active.isNotEmpty &&
      (intervals.isEmpty || intervals.last.endedAt != null)) {
    intervals.add(
      BrowserTakeoverInterval(
        startedAt: _date(active['startedAt']) ?? DateTime.now(),
        reason: active['reason']?.toString() ?? 'Manual inspection',
        openedHeaded: true,
      ),
    );
  }
  return intervals;
}

BrowserRuntimeStatus _runtimeStatus(Object? status, bool takeover) =>
    switch (status?.toString()) {
      'starting' => BrowserRuntimeStatus.launching,
      'running' when takeover => BrowserRuntimeStatus.paused,
      'running' => BrowserRuntimeStatus.running,
      'stopping' => BrowserRuntimeStatus.recovering,
      'failed' || 'orphaned' => BrowserRuntimeStatus.crashed,
      _ => BrowserRuntimeStatus.stopped,
    };

bool _isRefreshEvent(Map<String, dynamic> wire) {
  final kind = _envelope(wire)['kind']?.toString() ?? '';
  return kind.startsWith('browser.') &&
      kind != 'browser.event' &&
      kind != 'browser.serviceExited';
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

String _artifactLabel(String kind) => switch (kind) {
  'screenshot' => 'Browser screenshot',
  'snapshot' => 'Accessibility snapshot',
  'trace' => 'Browser trace',
  'logs' => 'Bounded browser logs',
  _ => 'Browser artifact · $kind',
};

bool _validUuid(String? value) => value != null && _uuid.hasMatch(value);

final _artifactHash = RegExp(r'^[0-9a-f]{64}$');
final _uuid = RegExp(
  r'^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-5][0-9a-fA-F]{3}-'
  r'[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$',
);
