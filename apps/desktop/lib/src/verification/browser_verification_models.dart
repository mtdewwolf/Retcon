enum BrowserStepKind {
  navigate,
  click,
  fill,
  press,
  waitFor,
  screenshot,
  accessibility,
}

enum BrowserAssertionKind {
  text,
  element,
  statusCode,
  console,
  screenshot,
  accessibility,
}

enum BrowserRunStatus {
  queued,
  running,
  needsReview,
  passed,
  failed,
  cancelled,
}

enum BrowserTimelineKind {
  navigation,
  interaction,
  network,
  console,
  screenshot,
  assertion,
  accessibility,
  error,
  takeover,
  completion,
}

enum VisualComparisonMethod { pixel, perceptual }

enum VisualComparisonStatus { passed, changed, missingBaseline, approved }

enum AccessibilitySeverity { info, warning, critical }

enum AccessibilityCategory {
  labels,
  contrast,
  keyboard,
  headings,
  landmarks,
  forms,
  focus,
}

class BrowserViewport {
  const BrowserViewport({
    required this.id,
    required this.label,
    required this.width,
    required this.height,
    this.deviceScaleFactor = 1,
  });

  final String id;
  final String label;
  final int width;
  final int height;
  final double deviceScaleFactor;

  String get dimensions => '${width}x$height @ ${deviceScaleFactor}x';
}

class BrowserAssertion {
  const BrowserAssertion({
    required this.id,
    required this.kind,
    required this.expected,
    this.target,
    this.required = true,
  });

  final String id;
  final BrowserAssertionKind kind;
  final String? target;
  final String expected;
  final bool required;

  BrowserAssertion copyWith({
    BrowserAssertionKind? kind,
    String? target,
    String? expected,
    bool? required,
  }) => BrowserAssertion(
    id: id,
    kind: kind ?? this.kind,
    target: target ?? this.target,
    expected: expected ?? this.expected,
    required: required ?? this.required,
  );
}

class BrowserVerificationStep {
  const BrowserVerificationStep({
    required this.id,
    required this.label,
    required this.kind,
    this.target,
    this.value,
    this.timeout = const Duration(seconds: 10),
    this.assertions = const [],
    this.enabled = true,
  });

  final String id;
  final String label;
  final BrowserStepKind kind;
  final String? target;
  final String? value;
  final Duration timeout;
  final List<BrowserAssertion> assertions;
  final bool enabled;

  BrowserVerificationStep copyWith({
    String? label,
    BrowserStepKind? kind,
    String? target,
    String? value,
    Duration? timeout,
    List<BrowserAssertion>? assertions,
    bool? enabled,
  }) => BrowserVerificationStep(
    id: id,
    label: label ?? this.label,
    kind: kind ?? this.kind,
    target: target ?? this.target,
    value: value ?? this.value,
    timeout: timeout ?? this.timeout,
    assertions: assertions ?? this.assertions,
    enabled: enabled ?? this.enabled,
  );
}

class BrowserVerificationDefinition {
  const BrowserVerificationDefinition({
    required this.id,
    required this.taskId,
    required this.name,
    required this.targetUrl,
    this.requiredServerId,
    this.viewports = const [
      BrowserViewport(
        id: 'desktop',
        label: 'Desktop',
        width: 1440,
        height: 900,
      ),
    ],
    this.timeout = const Duration(seconds: 30),
    this.retryCount = 1,
    this.visualThreshold = 0.01,
    this.maskSelectors = const [],
    this.ignoreSelectors = const [],
    this.steps = const [],
    this.required = true,
    this.failOnAccessibility = true,
    this.enabled = true,
    this.updatedAt,
  });

  final String id;
  final String taskId;
  final String name;
  final String targetUrl;
  final String? requiredServerId;
  final List<BrowserViewport> viewports;
  final Duration timeout;
  final int retryCount;
  final double visualThreshold;
  final List<String> maskSelectors;
  final List<String> ignoreSelectors;
  final List<BrowserVerificationStep> steps;
  final bool required;
  final bool failOnAccessibility;
  final bool enabled;
  final DateTime? updatedAt;

  BrowserVerificationDefinition copyWith({
    String? name,
    String? targetUrl,
    String? requiredServerId,
    List<BrowserViewport>? viewports,
    Duration? timeout,
    int? retryCount,
    double? visualThreshold,
    List<String>? maskSelectors,
    List<String>? ignoreSelectors,
    List<BrowserVerificationStep>? steps,
    bool? required,
    bool? failOnAccessibility,
    bool? enabled,
    DateTime? updatedAt,
  }) => BrowserVerificationDefinition(
    id: id,
    taskId: taskId,
    name: name ?? this.name,
    targetUrl: targetUrl ?? this.targetUrl,
    requiredServerId: requiredServerId ?? this.requiredServerId,
    viewports: viewports ?? this.viewports,
    timeout: timeout ?? this.timeout,
    retryCount: retryCount ?? this.retryCount,
    visualThreshold: visualThreshold ?? this.visualThreshold,
    maskSelectors: maskSelectors ?? this.maskSelectors,
    ignoreSelectors: ignoreSelectors ?? this.ignoreSelectors,
    steps: steps ?? this.steps,
    required: required ?? this.required,
    failOnAccessibility: failOnAccessibility ?? this.failOnAccessibility,
    enabled: enabled ?? this.enabled,
    updatedAt: updatedAt ?? this.updatedAt,
  );
}

class BrowserTimelineEvent {
  const BrowserTimelineEvent({
    required this.id,
    required this.kind,
    required this.label,
    required this.createdAt,
    this.details = const {},
    this.passed,
    this.duration,
  });

  final String id;
  final BrowserTimelineKind kind;
  final String label;
  final DateTime createdAt;
  final Map<String, Object?> details;
  final bool? passed;
  final Duration? duration;
}

class VisualMaskRegion {
  const VisualMaskRegion({
    required this.id,
    required this.label,
    required this.x,
    required this.y,
    required this.width,
    required this.height,
  });

  final String id;
  final String label;
  final double x;
  final double y;
  final double width;
  final double height;
}

class BrowserVisualArtifact {
  const BrowserVisualArtifact({required this.hash, this.localPath});
  final String hash;
  final String? localPath;
}

class BrowserVisualComparison {
  const BrowserVisualComparison({
    required this.id,
    required this.viewport,
    required this.method,
    required this.status,
    required this.threshold,
    required this.difference,
    required this.createdAt,
    this.baseline,
    this.current,
    this.diff,
    this.masks = const [],
    this.approvedBy,
    this.approvedAt,
  });

  final String id;
  final BrowserViewport viewport;
  final VisualComparisonMethod method;
  final VisualComparisonStatus status;
  final double threshold;
  final double difference;
  final DateTime createdAt;
  final BrowserVisualArtifact? baseline;
  final BrowserVisualArtifact? current;
  final BrowserVisualArtifact? diff;
  final List<VisualMaskRegion> masks;
  final String? approvedBy;
  final DateTime? approvedAt;

  bool get changed => status == VisualComparisonStatus.changed;

  BrowserVisualComparison copyWith({
    VisualComparisonStatus? status,
    String? approvedBy,
    DateTime? approvedAt,
  }) => BrowserVisualComparison(
    id: id,
    viewport: viewport,
    method: method,
    status: status ?? this.status,
    threshold: threshold,
    difference: difference,
    createdAt: createdAt,
    baseline: baseline,
    current: current,
    diff: diff,
    masks: masks,
    approvedBy: approvedBy ?? this.approvedBy,
    approvedAt: approvedAt ?? this.approvedAt,
  );
}

class BrowserAccessibilityIssue {
  const BrowserAccessibilityIssue({
    required this.id,
    required this.category,
    required this.severity,
    required this.message,
    this.selector,
    this.help,
  });

  final String id;
  final AccessibilityCategory category;
  final AccessibilitySeverity severity;
  final String message;
  final String? selector;
  final String? help;
}

class BrowserVerificationRun {
  const BrowserVerificationRun({
    required this.id,
    required this.taskId,
    required this.definitionId,
    required this.status,
    required this.startedAt,
    this.completedAt,
    this.attempt = 1,
    this.timeline = const [],
    this.visualComparisons = const [],
    this.accessibilityIssues = const [],
    this.consoleErrors = const [],
    this.warningMessages = const [],
  });

  final String id;
  final String taskId;
  final String definitionId;
  final BrowserRunStatus status;
  final DateTime startedAt;
  final DateTime? completedAt;
  final int attempt;
  final List<BrowserTimelineEvent> timeline;
  final List<BrowserVisualComparison> visualComparisons;
  final List<BrowserAccessibilityIssue> accessibilityIssues;
  final List<String> consoleErrors;
  final List<String> warningMessages;

  bool get running => status == BrowserRunStatus.running;
  bool get hasCriticalAccessibility => accessibilityIssues.any(
    (issue) => issue.severity == AccessibilitySeverity.critical,
  );
  bool get hasUnapprovedVisualChanges => visualComparisons.any(
    (comparison) => comparison.status == VisualComparisonStatus.changed,
  );
  bool get hasBlockingEvidence =>
      status != BrowserRunStatus.passed ||
      hasCriticalAccessibility ||
      hasUnapprovedVisualChanges ||
      consoleErrors.isNotEmpty;

  BrowserVerificationRun copyWith({
    BrowserRunStatus? status,
    DateTime? completedAt,
    List<BrowserTimelineEvent>? timeline,
    List<BrowserVisualComparison>? visualComparisons,
    List<BrowserAccessibilityIssue>? accessibilityIssues,
    List<String>? consoleErrors,
    List<String>? warningMessages,
  }) => BrowserVerificationRun(
    id: id,
    taskId: taskId,
    definitionId: definitionId,
    status: status ?? this.status,
    startedAt: startedAt,
    completedAt: completedAt ?? this.completedAt,
    attempt: attempt,
    timeline: timeline ?? this.timeline,
    visualComparisons: visualComparisons ?? this.visualComparisons,
    accessibilityIssues: accessibilityIssues ?? this.accessibilityIssues,
    consoleErrors: consoleErrors ?? this.consoleErrors,
    warningMessages: warningMessages ?? this.warningMessages,
  );
}

sealed class BrowserVerificationEvent {
  const BrowserVerificationEvent({required this.taskId, required this.runId});
  final String taskId;
  final String runId;
}

class BrowserVerificationRunUpdated extends BrowserVerificationEvent {
  const BrowserVerificationRunUpdated({
    required super.taskId,
    required super.runId,
    required this.run,
  });
  final BrowserVerificationRun run;
}
