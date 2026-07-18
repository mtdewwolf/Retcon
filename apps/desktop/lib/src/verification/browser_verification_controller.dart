import 'dart:async';

import 'package:flutter/foundation.dart';

import 'browser_verification_models.dart';
import 'browser_verification_repository.dart';

class BrowserVerificationController extends ChangeNotifier {
  BrowserVerificationController({
    required BrowserVerificationRepository repository,
    required this.taskId,
    this.devServerInstanceId,
  }) : _repository = repository {
    _events = repository.events
        .where((event) => event.taskId == taskId)
        .listen(_onEvent);
  }

  final BrowserVerificationRepository _repository;
  final String taskId;

  /// Returns the live instance for [requiredConfigId], or null when that
  /// configured server is not currently available.
  final String? Function(String? requiredConfigId)? devServerInstanceId;
  StreamSubscription<BrowserVerificationEvent>? _events;

  BrowserVerificationDefinition? definition;
  BrowserVerificationRun? activeRun;
  List<BrowserVerificationRun> history = const [];
  bool loading = false;
  bool saving = false;
  bool loaded = false;
  bool loadFailed = false;
  String? error;

  BrowserVerificationRun? get latestRun =>
      activeRun ?? (history.isEmpty ? null : history.first);
  bool get configured => definition?.enabled == true;
  bool get required => configured && definition!.required;
  bool get running => activeRun?.running == true;
  bool get canRun =>
      configured &&
      !running &&
      (devServerInstanceId == null ||
          devServerInstanceId!(definition?.requiredServerId) != null);
  bool get allowsCompletion {
    if (!loaded) return false;
    if (loadFailed) return false;
    if (!required) return true;
    final latest = latestRun;
    return latest != null &&
        latest.status == BrowserRunStatus.passed &&
        !latest.hasUnapprovedVisualChanges &&
        latest.consoleErrors.isEmpty &&
        (definition?.failOnAccessibility != true ||
            !latest.hasCriticalAccessibility);
  }

  String? get completionBlocker {
    if (!loaded) return 'Browser verification configuration is loading.';
    if (loadFailed) {
      return 'Browser verification evidence could not be loaded.';
    }
    if (!required || allowsCompletion) return null;
    final latest = latestRun;
    if (running) return 'Required browser verification is still running.';
    if (latest == null) return 'Run the required browser verification.';
    if (definition?.failOnAccessibility == true &&
        latest.hasCriticalAccessibility) {
      return 'Critical accessibility issues block task completion.';
    }
    if (latest.hasUnapprovedVisualChanges) {
      return 'Review or approve the visual differences.';
    }
    if (latest.consoleErrors.isNotEmpty) {
      return 'Browser console errors block task completion.';
    }
    if (latest.status == BrowserRunStatus.needsReview) {
      return 'Review the browser evidence before task completion.';
    }
    return 'Required browser verification failed.';
  }

  int get warningCount =>
      (latestRun?.accessibilityIssues
              .where((issue) => issue.severity == AccessibilitySeverity.warning)
              .length ??
          0) +
      (latestRun?.warningMessages.length ?? 0);

  Future<void> load() async {
    loading = true;
    loadFailed = false;
    error = null;
    notifyListeners();
    try {
      final values = await Future.wait([
        _repository.loadDefinition(taskId),
        _repository.listHistory(taskId),
      ]);
      definition = values[0] as BrowserVerificationDefinition?;
      final loadedHistory = values[1] as List<BrowserVerificationRun>;
      final definitionUpdatedAt = definition?.updatedAt;
      history = definitionUpdatedAt == null
          ? loadedHistory
          : loadedHistory
                .where((run) => !run.startedAt.isBefore(definitionUpdatedAt))
                .toList();
      loaded = true;
    } on Object catch (caught) {
      error = caught.toString();
      loadFailed = true;
      loaded = true;
    } finally {
      loading = false;
      notifyListeners();
    }
  }

  Future<void> saveDefinition(BrowserVerificationDefinition value) async {
    final validation = _validateDefinition(value);
    if (validation != null) {
      error = validation;
      notifyListeners();
      return;
    }
    saving = true;
    error = null;
    notifyListeners();
    try {
      definition = await _repository.saveDefinition(value);
      activeRun = null;
      history = const [];
    } on Object catch (caught) {
      error = caught.toString();
    } finally {
      saving = false;
      notifyListeners();
    }
  }

  Future<void> run() async {
    final value = definition;
    if (value == null || !value.enabled || running) return;
    final validation = _validateDefinition(value);
    if (validation != null) {
      error = validation;
      notifyListeners();
      return;
    }
    final instanceId = devServerInstanceId?.call(value.requiredServerId);
    if (devServerInstanceId != null && instanceId == null) {
      error = 'Start the required development server before verification.';
      notifyListeners();
      return;
    }
    error = null;
    try {
      activeRun = await _repository.startRun(
        value,
        devServerInstanceId: instanceId,
      );
    } on Object catch (caught) {
      error = caught.toString();
    }
    notifyListeners();
  }

  Future<void> cancel() async {
    final run = activeRun;
    if (run == null) return;
    await _repository.cancelRun(run.id);
  }

  Future<void> approveBaseline(BrowserVisualComparison comparison) async {
    final run = latestRun;
    if (run == null) return;
    try {
      final approved = await _repository.approveBaseline(run.id, comparison.id);
      final updated = run.copyWith(
        visualComparisons: run.visualComparisons
            .map((item) => item.id == comparison.id ? approved : item)
            .toList(),
      );
      if (activeRun?.id == run.id) activeRun = updated;
      history = history
          .map((item) => item.id == run.id ? updated : item)
          .toList();
      notifyListeners();
    } on Object catch (caught) {
      error = caught.toString();
      notifyListeners();
    }
  }

  Future<void> review({required bool approve, String reason = ''}) async {
    final run = latestRun;
    if (run == null || run.status != BrowserRunStatus.needsReview) return;
    try {
      final reviewed = await _repository.reviewRun(
        run.id,
        approve: approve,
        reason: reason,
      );
      activeRun = reviewed.running ? reviewed : null;
      history = [reviewed, ...history.where((item) => item.id != reviewed.id)];
      notifyListeners();
    } on Object catch (caught) {
      error = caught.toString();
      notifyListeners();
    }
  }

  void _onEvent(BrowserVerificationEvent event) {
    if (event case BrowserVerificationRunUpdated(:final run)) {
      if (run.running) {
        activeRun = run;
      } else {
        activeRun = null;
        history = [run, ...history.where((item) => item.id != run.id)];
      }
      notifyListeners();
    }
  }

  String? _validateDefinition(BrowserVerificationDefinition value) {
    final url = Uri.tryParse(value.targetUrl);
    if (url == null ||
        !url.hasAuthority ||
        (url.scheme != 'http' && url.scheme != 'https')) {
      return 'Enter a valid HTTP or HTTPS target URL.';
    }
    if (value.timeout.inMilliseconds < 500 ||
        value.timeout.inMilliseconds > 120000) {
      return 'Timeout must be between 0.5 and 120 seconds.';
    }
    if (value.retryCount < 0 || value.retryCount > 3) {
      return 'Retries must be between 0 and 3.';
    }
    if (value.visualThreshold < 0 || value.visualThreshold > 1) {
      return 'Visual difference threshold must be between 0% and 100%.';
    }
    if (value.viewports.isEmpty ||
        value.viewports.any(
          (viewport) => viewport.width <= 0 || viewport.height <= 0,
        )) {
      return 'Add at least one valid responsive viewport.';
    }
    const elementStates = {
      'attached',
      'detached',
      'visible',
      'hidden',
      'enabled',
      'disabled',
    };
    for (final step in value.steps) {
      if (!step.enabled) continue;
      if ((step.kind == BrowserStepKind.navigate ||
              step.kind == BrowserStepKind.click ||
              step.kind == BrowserStepKind.fill) &&
          (step.target == null || step.target!.trim().isEmpty)) {
        return '${step.label} needs a URL or selector target.';
      }
      if (step.kind == BrowserStepKind.fill &&
          (step.value == null || step.value!.isEmpty)) {
        return '${step.label} needs a fill value.';
      }
      for (final assertion in step.assertions) {
        if (assertion.expected.trim().isEmpty) {
          return 'Every assertion needs an explicit expected value.';
        }
        if (assertion.kind == BrowserAssertionKind.statusCode) {
          final status = int.tryParse(assertion.expected);
          if (status == null || status < 100 || status > 599) {
            return 'Status-code assertions must be between 100 and 599.';
          }
        }
        if (assertion.kind == BrowserAssertionKind.element &&
            !elementStates.contains(assertion.expected.toLowerCase())) {
          return 'Element assertions must expect attached, detached, visible, '
              'hidden, enabled, or disabled.';
        }
        if ((assertion.kind == BrowserAssertionKind.text ||
                assertion.kind == BrowserAssertionKind.element) &&
            (assertion.target == null || assertion.target!.trim().isEmpty)) {
          return 'Text and element assertions need a selector target.';
        }
      }
    }
    return null;
  }

  @override
  void dispose() {
    unawaited(_events?.cancel());
    super.dispose();
  }
}
