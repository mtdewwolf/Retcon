import 'dart:async';

import 'package:flutter/foundation.dart';

import 'verification_models.dart';
import 'verification_repository.dart';

class VerificationController extends ChangeNotifier {
  VerificationController({
    required VerificationRepository repository,
    required this.taskId,
    this.projectId,
    this.projectPath,
    this.maxLogCharacters = 16000,
  }) : _repository = repository {
    _events = repository.events
        .where((event) => event.taskId == taskId)
        .listen(_onEvent);
  }

  final VerificationRepository _repository;
  final String taskId;
  final String? projectId;
  final String? projectPath;
  final int maxLogCharacters;
  StreamSubscription<VerificationEvent>? _events;

  List<ProjectCommand> commands = const [];
  List<VerificationGate> gates = const [];
  List<VerificationRun> history = const [];
  VerificationRun? activeRun;
  VerificationCompletionReport? report;
  bool loading = false;
  bool loaded = false;
  String? error;
  String? selectedOutputGateId;

  bool get running => activeRun?.status == VerificationRunStatus.running;
  VerificationRun? get latestRun => history.isEmpty ? null : history.first;
  bool get hasRequiredGates =>
      gates.any((gate) => gate.enabled && gate.required);
  bool get allowsCompletion {
    if (!loaded) return false;
    if (!hasRequiredGates) return true;
    final requiredIds = gates
        .where((gate) => gate.enabled && gate.required)
        .map((gate) => gate.id)
        .toSet();
    final active = activeRun;
    if (active?.status == VerificationRunStatus.running &&
        active!.gates.any(
          (gate) =>
              requiredIds.contains(gate.gateId) &&
              gate.status != GateStatus.passed,
        )) {
      return false;
    }
    return requiredIds.every((id) {
      for (final run in history) {
        for (final gate in run.gates) {
          if (gate.gateId == id) return gate.status == GateStatus.passed;
        }
      }
      return false;
    });
  }

  String? get completionBlocker {
    if (!loaded) return 'Verification configuration is loading.';
    if (!hasRequiredGates || allowsCompletion) return null;
    if (running) return 'Required verification gates are still running.';
    if (latestRun == null) return 'Run all required verification gates.';
    return 'One or more required verification gates failed.';
  }

  VerificationComparison? get comparison => history.length < 2
      ? null
      : VerificationComparison(current: history[0], previous: history[1]);

  Future<void> load() async {
    loading = true;
    error = null;
    notifyListeners();
    try {
      final values = await Future.wait([
        _repository.detectCommands(
          projectId: projectId,
          projectPath: projectPath,
        ),
        _repository.loadGates(taskId, projectId: projectId),
        _repository.listHistory(taskId),
      ]);
      commands = values[0] as List<ProjectCommand>;
      gates = values[1] as List<VerificationGate>;
      history = (values[2] as List<VerificationRun>).map(_boundRun).toList();
      report = latestRun == null
          ? null
          : await _repository.loadReport(latestRun!.id);
      selectedOutputGateId = latestRun?.gates.firstOrNull?.gateId;
      loaded = true;
    } on Object catch (caught) {
      error = caught.toString();
    } finally {
      loading = false;
      notifyListeners();
    }
  }

  Future<void> detectCommands() async {
    commands = await _repository.detectCommands(
      projectId: projectId,
      projectPath: projectPath,
    );
    notifyListeners();
  }

  Future<void> overrideCommand(ProjectCommand command, String value) async {
    final saved = await _repository.saveCommandOverride(
      command.copyWith(command: value.trim(), source: CommandSource.override),
      projectId: projectId,
    );
    commands = commands
        .map((item) => item.id == saved.id ? saved : item)
        .toList();
    notifyListeners();
  }

  Future<void> addGate(ProjectCommand command) async {
    if (gates.any((gate) => gate.id == command.id)) return;
    gates = await _repository.saveGates(taskId, [
      ...gates,
      VerificationGate(
        id: command.id,
        label: command.label,
        command: command.command,
        kind: command.kind,
        cwd: command.cwd,
        timeout: command.timeout,
      ),
    ], projectId: projectId);
    notifyListeners();
  }

  Future<void> updateGate(VerificationGate gate) async {
    gates = await _repository.saveGates(
      taskId,
      gates.map((item) => item.id == gate.id ? gate : item).toList(),
      projectId: projectId,
    );
    notifyListeners();
  }

  Future<void> removeGate(String gateId) async {
    gates = await _repository.saveGates(
      taskId,
      gates.where((gate) => gate.id != gateId).toList(),
      projectId: projectId,
    );
    notifyListeners();
  }

  Future<void> moveGate(String gateId, int delta) async {
    final oldIndex = gates.indexWhere((gate) => gate.id == gateId);
    final newIndex = oldIndex + delta;
    if (oldIndex < 0 || newIndex < 0 || newIndex >= gates.length) return;
    final reordered = [...gates];
    final gate = reordered.removeAt(oldIndex);
    reordered.insert(newIndex, gate);
    gates = await _repository.saveGates(
      taskId,
      reordered,
      projectId: projectId,
    );
    notifyListeners();
  }

  Future<void> runAll() => _start();

  Future<void> rerunFailed() async {
    final latest = latestRun;
    if (latest == null ||
        !latest.gates.any((gate) => gate.status == GateStatus.failed) ||
        running) {
      return;
    }
    error = null;
    try {
      activeRun = await _repository.rerunRun(latest.id);
      selectedOutputGateId = activeRun?.gates.firstOrNull?.gateId;
    } on Object catch (caught) {
      error = caught.toString();
    }
    notifyListeners();
  }

  Future<void> _start({Set<String>? gateIds}) async {
    if (running || gates.where((gate) => gate.enabled).isEmpty) return;
    error = null;
    try {
      activeRun = await _repository.startRun(taskId, gates, gateIds: gateIds);
      selectedOutputGateId = activeRun?.gates.firstOrNull?.gateId;
    } on Object catch (caught) {
      error = caught.toString();
    }
    notifyListeners();
  }

  Future<void> cancel() async {
    final run = activeRun;
    if (run != null && running) await _repository.cancelRun(run.id);
  }

  void selectOutput(String gateId) {
    selectedOutputGateId = gateId;
    notifyListeners();
  }

  GateExecution? get selectedOutput {
    final run = activeRun ?? latestRun;
    for (final gate in run?.gates ?? const <GateExecution>[]) {
      if (gate.gateId == selectedOutputGateId) return gate;
    }
    return null;
  }

  void _onEvent(VerificationEvent event) {
    if (event is RunUpdated) {
      activeRun = _boundRun(event.run);
      if (event.run.status != VerificationRunStatus.running) {
        unawaited(_refreshHistory());
      }
      notifyListeners();
      return;
    }
    final run = activeRun;
    if (run == null || run.id != event.runId) {
      if (event is RunFinished) unawaited(_refreshHistory());
      return;
    }
    switch (event) {
      case RunStarted():
        break;
      case GateStarted(:final gateId):
        activeRun = run.copyWith(
          gates: run.gates
              .map(
                (gate) => gate.gateId == gateId
                    ? gate.copyWith(status: GateStatus.running)
                    : gate,
              )
              .toList(),
        );
      case GateOutput(:final gateId, :final text, :final stderr):
        activeRun = run.copyWith(
          gates: run.gates.map((gate) {
            if (gate.gateId != gateId) return gate;
            return stderr
                ? gate.copyWith(stderr: _bounded('${gate.stderr}$text'))
                : gate.copyWith(stdout: _bounded('${gate.stdout}$text'));
          }).toList(),
        );
      case GateFinished(:final execution):
        final prior = run.gates.firstWhere(
          (gate) => gate.gateId == execution.gateId,
        );
        activeRun = run.copyWith(
          gates: run.gates
              .map(
                (gate) => gate.gateId == execution.gateId
                    ? execution.copyWith(
                        stdout: _bounded('${prior.stdout}${execution.stdout}'),
                        stderr: _bounded('${prior.stderr}${execution.stderr}'),
                      )
                    : gate,
              )
              .toList(),
        );
      case RunFinished(:final run):
        activeRun = _boundRun(run);
        unawaited(_refreshHistory());
      case RunUpdated():
        break;
    }
    notifyListeners();
  }

  String _bounded(String value) => value.length <= maxLogCharacters
      ? value
      : '…${value.substring(value.length - maxLogCharacters)}';

  VerificationRun _boundRun(VerificationRun run) => run.copyWith(
    gates: run.gates
        .map(
          (gate) => gate.copyWith(
            stdout: _bounded(gate.stdout),
            stderr: _bounded(gate.stderr),
          ),
        )
        .toList(),
  );

  Future<void> _refreshHistory() async {
    history = (await _repository.listHistory(taskId)).map(_boundRun).toList();
    report = latestRun == null
        ? null
        : await _repository.loadReport(latestRun!.id);
    notifyListeners();
  }

  @override
  void dispose() {
    unawaited(_events?.cancel());
    super.dispose();
  }
}
