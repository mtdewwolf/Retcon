import 'dart:async';

import 'browser_verification_models.dart';

abstract interface class BrowserVerificationRepository {
  Stream<BrowserVerificationEvent> get events;

  Future<BrowserVerificationDefinition?> loadDefinition(String taskId);
  Future<BrowserVerificationDefinition> saveDefinition(
    BrowserVerificationDefinition definition,
  );
  Future<BrowserVerificationRun> startRun(
    BrowserVerificationDefinition definition, {
    String? devServerInstanceId,
  });
  Future<void> cancelRun(String runId);
  Future<BrowserVerificationRun> reviewRun(
    String runId, {
    required bool approve,
    String reason = '',
  });
  Future<List<BrowserVerificationRun>> listHistory(
    String taskId, {
    int limit = 20,
  });
  Future<BrowserVisualComparison> approveBaseline(
    String runId,
    String comparisonId,
  );
}

class ScriptedBrowserVerificationResult {
  const ScriptedBrowserVerificationResult({
    this.status = BrowserRunStatus.passed,
    this.timeline = const [],
    this.visualComparisons = const [],
    this.accessibilityIssues = const [],
    this.consoleErrors = const [],
    this.warningMessages = const [],
  });

  final BrowserRunStatus status;
  final List<BrowserTimelineEvent> timeline;
  final List<BrowserVisualComparison> visualComparisons;
  final List<BrowserAccessibilityIssue> accessibilityIssues;
  final List<String> consoleErrors;
  final List<String> warningMessages;
}

class InMemoryBrowserVerificationRepository
    implements BrowserVerificationRepository {
  InMemoryBrowserVerificationRepository({
    Map<String, BrowserVerificationDefinition> definitions = const {},
    Map<String, List<BrowserVerificationRun>> history = const {},
    Map<String, ScriptedBrowserVerificationResult> results = const {},
    this.autoComplete = true,
  }) : _definitions = {...definitions},
       _history = {
         for (final entry in history.entries) entry.key: [...entry.value],
       },
       _results = {...results};

  final bool autoComplete;
  final Map<String, BrowserVerificationDefinition> _definitions;
  final Map<String, List<BrowserVerificationRun>> _history;
  final Map<String, ScriptedBrowserVerificationResult> _results;
  final Map<String, BrowserVerificationRun> _active = {};
  final _events = StreamController<BrowserVerificationEvent>.broadcast();
  int _nextId = 0;

  @override
  Stream<BrowserVerificationEvent> get events => _events.stream;

  @override
  Future<BrowserVerificationDefinition?> loadDefinition(String taskId) async =>
      _definitions[taskId];

  @override
  Future<BrowserVerificationDefinition> saveDefinition(
    BrowserVerificationDefinition definition,
  ) async {
    _definitions[definition.taskId] = definition;
    return definition;
  }

  @override
  Future<BrowserVerificationRun> startRun(
    BrowserVerificationDefinition definition, {
    String? devServerInstanceId,
  }) async {
    final run = BrowserVerificationRun(
      id: 'browser-verification-${++_nextId}',
      taskId: definition.taskId,
      definitionId: definition.id,
      status: BrowserRunStatus.running,
      startedAt: DateTime.now(),
      timeline: [
        BrowserTimelineEvent(
          id: 'run-started-$_nextId',
          kind: BrowserTimelineKind.navigation,
          label: 'Opened ${definition.targetUrl}',
          createdAt: DateTime.now(),
          details: {'url': definition.targetUrl},
        ),
      ],
    );
    _active[run.id] = run;
    scheduleMicrotask(() => _emit(run));
    if (autoComplete) scheduleMicrotask(() => _complete(run));
    return run;
  }

  void _complete(BrowserVerificationRun run) {
    if (!_active.containsKey(run.id)) return;
    final result =
        _results[run.definitionId] ?? const ScriptedBrowserVerificationResult();
    final completed = run.copyWith(
      status: result.status,
      completedAt: DateTime.now(),
      timeline: [
        ...run.timeline,
        ...result.timeline,
        BrowserTimelineEvent(
          id: '${run.id}-completed',
          kind: BrowserTimelineKind.completion,
          label: result.status == BrowserRunStatus.passed
              ? 'Browser verification passed'
              : 'Browser verification failed',
          createdAt: DateTime.now(),
          passed: result.status == BrowserRunStatus.passed,
        ),
      ],
      visualComparisons: result.visualComparisons,
      accessibilityIssues: result.accessibilityIssues,
      consoleErrors: result.consoleErrors,
      warningMessages: result.warningMessages,
    );
    _active.remove(run.id);
    _history.putIfAbsent(run.taskId, () => []).insert(0, completed);
    _emit(completed);
  }

  void _emit(BrowserVerificationRun run) => _events.add(
    BrowserVerificationRunUpdated(taskId: run.taskId, runId: run.id, run: run),
  );

  @override
  Future<void> cancelRun(String runId) async {
    final run = _active.remove(runId);
    if (run == null) return;
    final cancelled = run.copyWith(
      status: BrowserRunStatus.cancelled,
      completedAt: DateTime.now(),
    );
    _history.putIfAbsent(run.taskId, () => []).insert(0, cancelled);
    _emit(cancelled);
  }

  @override
  Future<BrowserVerificationRun> reviewRun(
    String runId, {
    required bool approve,
    String reason = '',
  }) async {
    for (final entry in _history.entries) {
      final index = entry.value.indexWhere((run) => run.id == runId);
      if (index < 0) continue;
      final reviewed = entry.value[index].copyWith(
        status: approve ? BrowserRunStatus.passed : BrowserRunStatus.failed,
      );
      entry.value[index] = reviewed;
      _emit(reviewed);
      return reviewed;
    }
    throw StateError('Browser verification run not found.');
  }

  @override
  Future<List<BrowserVerificationRun>> listHistory(
    String taskId, {
    int limit = 20,
  }) async => [...?_history[taskId]].take(limit).toList();

  @override
  Future<BrowserVisualComparison> approveBaseline(
    String runId,
    String comparisonId,
  ) async {
    BrowserVerificationRun? located;
    for (final runs in _history.values) {
      final matches = runs.where((run) => run.id == runId);
      if (matches.isNotEmpty) located = matches.first;
    }
    if (located == null) {
      throw StateError('Browser verification run not found.');
    }
    final comparison = located.visualComparisons.firstWhere(
      (item) => item.id == comparisonId,
    );
    final approved = comparison.copyWith(
      status: VisualComparisonStatus.approved,
      approvedBy: 'user',
      approvedAt: DateTime.now(),
    );
    final updated = located.copyWith(
      visualComparisons: located.visualComparisons
          .map((item) => item.id == comparisonId ? approved : item)
          .toList(),
    );
    final runs = _history[located.taskId]!;
    runs[runs.indexWhere((run) => run.id == located!.id)] = updated;
    _emit(updated);
    return approved;
  }
}
