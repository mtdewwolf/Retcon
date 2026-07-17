import 'dart:async';

import 'package:flutter/foundation.dart';

import 'browser_models.dart';
import 'browser_repository.dart';

class BrowserController extends ChangeNotifier {
  BrowserController({required BrowserRepository repository})
    : _repository = repository {
    _events = repository.events.listen(_accept);
  }

  final BrowserRepository _repository;
  StreamSubscription<BrowserSnapshot>? _events;
  BrowserSnapshot snapshot = const BrowserSnapshot();
  BrowserEvidenceKind evidenceKind = BrowserEvidenceKind.console;
  bool loading = false;
  bool loaded = false;
  bool busy = false;
  String? error;
  String? lastActionResult;

  BrowserCapabilities get capabilities => _repository.capabilities;
  BrowserSession? get session => snapshot.session;
  BrowserTab? get activeTab => session?.activeTab;
  BrowserRuntimeStatus get status =>
      session?.status ?? BrowserRuntimeStatus.stopped;
  bool get running =>
      status == BrowserRuntimeStatus.running ||
      status == BrowserRuntimeStatus.paused;
  bool get automationPaused => session?.automationPaused == true;
  bool get crashed => status == BrowserRuntimeStatus.crashed;
  String get address => activeTab?.url ?? '';

  List<BrowserEvidenceEntry> get visibleEvidence => snapshot.evidence
      .where((entry) => entry.kind == evidenceKind)
      .toList()
      .reversed
      .toList();

  Future<void> load() async {
    loading = true;
    error = null;
    notifyListeners();
    try {
      _accept(await _repository.load());
      loaded = true;
    } on Object catch (caught) {
      error = caught.toString();
    } finally {
      loading = false;
      notifyListeners();
    }
  }

  Future<void> launch() => _run(_repository.launch);
  Future<void> close() => _run(_repository.close);
  Future<void> recover() => _run(_repository.recover);
  Future<void> newTab() => _run(_repository.newTab);
  Future<void> closeTab(String tabId) =>
      _run(() => _repository.closeTab(tabId));
  Future<void> selectTab(String tabId) =>
      _run(() => _repository.selectTab(tabId));
  Future<void> navigate(
    String url, {
    Map<String, dynamic> metadata = const {},
  }) => _run(() => _repository.navigate(url, metadata: metadata));
  Future<void> openPreview(BrowserPreviewRequest preview) async {
    if (!running) await launch();
    if (running) {
      await navigate(preview.url, metadata: preview.metadata);
    }
  }

  Future<void> back() => _run(_repository.back);
  Future<void> forward() => _run(_repository.forward);
  Future<void> reload() => _run(_repository.reload);
  Future<void> stopLoading() => _run(_repository.stopLoading);
  Future<void> setViewport(BrowserViewport viewport) =>
      _run(() => _repository.setViewport(viewport));
  Future<void> captureScreenshot({bool fullPage = false}) =>
      _run(() => _repository.captureScreenshot(fullPage: fullPage));
  Future<void> refreshEvidence() => _run(_repository.refreshEvidence);

  Future<void> performAction(BrowserAutomationAction action) async {
    await _run(() => _repository.performAction(action));
    if (error == null) lastActionResult = '${action.kind.name} completed';
    notifyListeners();
  }

  Future<void> pauseAutomation({String reason = 'Manual inspection'}) =>
      _run(() => _repository.pauseAutomation(reason: reason));
  Future<void> openHeadedTakeover() => _run(_repository.openHeadedTakeover);
  Future<void> resumeAutomation() => _run(_repository.resumeAutomation);

  void showEvidence(BrowserEvidenceKind kind) {
    evidenceKind = kind;
    notifyListeners();
  }

  void clearError() {
    error = null;
    notifyListeners();
  }

  Future<void> _run(Future<BrowserSnapshot> Function() operation) async {
    if (busy) return;
    busy = true;
    error = null;
    notifyListeners();
    try {
      _accept(await operation());
    } on Object catch (caught) {
      error = caught.toString();
    } finally {
      busy = false;
      notifyListeners();
    }
  }

  void _accept(BrowserSnapshot value) {
    snapshot = value;
    notifyListeners();
  }

  @override
  void dispose() {
    unawaited(_events?.cancel());
    super.dispose();
  }
}
