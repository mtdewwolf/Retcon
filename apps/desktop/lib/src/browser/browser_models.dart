enum BrowserRuntimeStatus {
  stopped,
  launching,
  running,
  paused,
  crashed,
  recovering,
}

enum BrowserTabStatus { loading, ready, failed, crashed }

enum BrowserDevicePreset { responsive, desktop, tablet, mobile }

enum BrowserEvidenceKind {
  screenshots,
  console,
  network,
  errors,
  accessibility,
  performance,
  artifacts,
}

enum BrowserActionKind { click, fill, press, select, readText }

class BrowserCapabilities {
  const BrowserCapabilities({
    this.multipleTabs = true,
    this.historyNavigation = true,
    this.reload = true,
    this.stopLoading = true,
    this.viewportAndDevice = true,
    this.screenshots = true,
    this.consoleAndNetwork = true,
    this.accessibility = true,
    this.performance = true,
    this.automation = true,
    this.headedTakeover = true,
    this.cookiesAndStorage = true,
  });

  final bool multipleTabs;
  final bool historyNavigation;
  final bool reload;
  final bool stopLoading;
  final bool viewportAndDevice;
  final bool screenshots;
  final bool consoleAndNetwork;
  final bool accessibility;
  final bool performance;
  final bool automation;
  final bool headedTakeover;
  final bool cookiesAndStorage;
}

class BrowserHistoryEntry {
  const BrowserHistoryEntry({
    required this.kind,
    required this.actor,
    required this.createdAt,
    this.details = const {},
  });
  final String kind;
  final String actor;
  final DateTime createdAt;
  final Map<String, dynamic> details;
}

class BrowserViewport {
  const BrowserViewport({
    required this.width,
    required this.height,
    this.device = BrowserDevicePreset.responsive,
    this.deviceScaleFactor = 1,
  });

  final int width;
  final int height;
  final BrowserDevicePreset device;
  final double deviceScaleFactor;

  BrowserViewport copyWith({
    int? width,
    int? height,
    BrowserDevicePreset? device,
    double? deviceScaleFactor,
  }) => BrowserViewport(
    width: width ?? this.width,
    height: height ?? this.height,
    device: device ?? this.device,
    deviceScaleFactor: deviceScaleFactor ?? this.deviceScaleFactor,
  );
}

class BrowserTab {
  const BrowserTab({
    required this.id,
    required this.title,
    required this.url,
    this.status = BrowserTabStatus.ready,
    this.canGoBack = false,
    this.canGoForward = false,
  });

  final String id;
  final String title;
  final String url;
  final BrowserTabStatus status;
  final bool canGoBack;
  final bool canGoForward;

  BrowserTab copyWith({
    String? title,
    String? url,
    BrowserTabStatus? status,
    bool? canGoBack,
    bool? canGoForward,
  }) => BrowserTab(
    id: id,
    title: title ?? this.title,
    url: url ?? this.url,
    status: status ?? this.status,
    canGoBack: canGoBack ?? this.canGoBack,
    canGoForward: canGoForward ?? this.canGoForward,
  );
}

class BrowserEvidenceEntry {
  const BrowserEvidenceEntry({
    required this.kind,
    required this.summary,
    required this.createdAt,
    this.level = 'info',
    this.details = const {},
  });

  final BrowserEvidenceKind kind;
  final String summary;
  final DateTime createdAt;
  final String level;
  final Map<String, dynamic> details;
}

class BrowserArtifact {
  const BrowserArtifact({
    required this.id,
    required this.label,
    required this.path,
    required this.createdAt,
    this.metadata = const {},
  });

  final String id;
  final String label;
  final String path;
  final DateTime createdAt;
  final Map<String, dynamic> metadata;
}

class BrowserTakeoverInterval {
  const BrowserTakeoverInterval({
    required this.startedAt,
    required this.reason,
    this.endedAt,
    this.openedHeaded = false,
  });

  final DateTime startedAt;
  final DateTime? endedAt;
  final String reason;
  final bool openedHeaded;

  BrowserTakeoverInterval copyWith({DateTime? endedAt, bool? openedHeaded}) =>
      BrowserTakeoverInterval(
        startedAt: startedAt,
        reason: reason,
        endedAt: endedAt ?? this.endedAt,
        openedHeaded: openedHeaded ?? this.openedHeaded,
      );
}

class BrowserSession {
  const BrowserSession({
    required this.id,
    required this.profileId,
    required this.status,
    required this.tabs,
    required this.activeTabId,
    required this.viewport,
    this.headless = true,
    this.automationPaused = false,
    this.crashMessage,
    this.recoveryCount = 0,
    this.previewMetadata = const {},
    this.takeoverHistory = const [],
    this.history = const [],
  });

  final String id;
  final String profileId;
  final BrowserRuntimeStatus status;
  final List<BrowserTab> tabs;
  final String activeTabId;
  final BrowserViewport viewport;
  final bool headless;
  final bool automationPaused;
  final String? crashMessage;
  final int recoveryCount;
  final Map<String, dynamic> previewMetadata;
  final List<BrowserTakeoverInterval> takeoverHistory;
  final List<BrowserHistoryEntry> history;

  BrowserTab? get activeTab {
    for (final tab in tabs) {
      if (tab.id == activeTabId) return tab;
    }
    return tabs.isEmpty ? null : tabs.first;
  }

  BrowserSession copyWith({
    BrowserRuntimeStatus? status,
    List<BrowserTab>? tabs,
    String? activeTabId,
    BrowserViewport? viewport,
    bool? headless,
    bool? automationPaused,
    String? crashMessage,
    bool clearCrashMessage = false,
    int? recoveryCount,
    Map<String, dynamic>? previewMetadata,
    List<BrowserTakeoverInterval>? takeoverHistory,
    List<BrowserHistoryEntry>? history,
  }) => BrowserSession(
    id: id,
    profileId: profileId,
    status: status ?? this.status,
    tabs: tabs ?? this.tabs,
    activeTabId: activeTabId ?? this.activeTabId,
    viewport: viewport ?? this.viewport,
    headless: headless ?? this.headless,
    automationPaused: automationPaused ?? this.automationPaused,
    crashMessage: clearCrashMessage ? null : crashMessage ?? this.crashMessage,
    recoveryCount: recoveryCount ?? this.recoveryCount,
    previewMetadata: previewMetadata ?? this.previewMetadata,
    takeoverHistory: takeoverHistory ?? this.takeoverHistory,
    history: history ?? this.history,
  );
}

class BrowserSnapshot {
  const BrowserSnapshot({
    this.session,
    this.evidence = const [],
    this.artifacts = const [],
    this.screenshotPath,
  });

  final BrowserSession? session;
  final List<BrowserEvidenceEntry> evidence;
  final List<BrowserArtifact> artifacts;
  final String? screenshotPath;

  BrowserSnapshot copyWith({
    BrowserSession? session,
    bool clearSession = false,
    List<BrowserEvidenceEntry>? evidence,
    List<BrowserArtifact>? artifacts,
    String? screenshotPath,
  }) => BrowserSnapshot(
    session: clearSession ? null : session ?? this.session,
    evidence: evidence ?? this.evidence,
    artifacts: artifacts ?? this.artifacts,
    screenshotPath: screenshotPath ?? this.screenshotPath,
  );
}

class BrowserAutomationAction {
  const BrowserAutomationAction({
    required this.kind,
    required this.selector,
    this.value,
  });

  final BrowserActionKind kind;
  final String selector;
  final String? value;
}

class BrowserPreviewRequest {
  const BrowserPreviewRequest({required this.url, this.metadata = const {}});

  final String url;
  final Map<String, dynamic> metadata;
}
