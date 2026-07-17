import 'dart:async';

import 'verification_models.dart';

abstract interface class VerificationRepository {
  Stream<VerificationEvent> get events;
  Future<List<ProjectCommand>> detectCommands({
    String? projectId,
    String? projectPath,
  });
  Future<ProjectCommand> saveCommandOverride(
    ProjectCommand command, {
    String? projectId,
  });
  Future<List<VerificationGate>> loadGates(String taskId, {String? projectId});
  Future<List<VerificationGate>> saveGates(
    String taskId,
    List<VerificationGate> gates, {
    String? projectId,
  });
  Future<VerificationRun> startRun(
    String taskId,
    List<VerificationGate> gates, {
    Set<String>? gateIds,
  });
  Future<void> cancelRun(String runId);
  Future<VerificationRun> rerunRun(String runId);
  Future<List<VerificationRun>> listHistory(String taskId, {int limit = 20});
  Future<VerificationCompletionReport?> loadReport(String runId);
}

class ScriptedGateResult {
  const ScriptedGateResult({
    this.passed = true,
    this.duration = const Duration(milliseconds: 350),
    this.tests = const TestCounts(),
    this.fileLinks = const [],
    this.stdout = '',
    this.stderr = '',
  });
  final bool passed;
  final Duration duration;
  final TestCounts tests;
  final List<VerificationFileLink> fileLinks;
  final String stdout;
  final String stderr;
}

/// Stateful fake used until the verification RPC contract lands.
class InMemoryVerificationRepository implements VerificationRepository {
  InMemoryVerificationRepository({
    List<ProjectCommand> commands = const [],
    Map<String, List<VerificationGate>> gates = const {},
    Map<String, List<VerificationRun>> history = const {},
    Map<String, ScriptedGateResult> results = const {},
    this.autoComplete = true,
  }) : _commands = [...commands],
       _gates = {
         for (final entry in gates.entries) entry.key: [...entry.value],
       },
       _history = {
         for (final entry in history.entries) entry.key: [...entry.value],
       },
       _results = {...results};

  factory InMemoryVerificationRepository.demo() =>
      InMemoryVerificationRepository(
        commands: const [
          ProjectCommand(
            id: 'analyze',
            label: 'Static analysis',
            command: 'flutter analyze',
          ),
          ProjectCommand(id: 'test', label: 'Tests', command: 'flutter test'),
        ],
      );

  final bool autoComplete;
  final List<ProjectCommand> _commands;
  final Map<String, List<VerificationGate>> _gates;
  final Map<String, List<VerificationRun>> _history;
  final Map<String, ScriptedGateResult> _results;
  final _events = StreamController<VerificationEvent>.broadcast();
  final Map<String, VerificationRun> _active = {};
  int _nextId = 0;

  @override
  Stream<VerificationEvent> get events => _events.stream;

  @override
  Future<List<ProjectCommand>> detectCommands({
    String? projectId,
    String? projectPath,
  }) async => [..._commands];

  @override
  Future<ProjectCommand> saveCommandOverride(
    ProjectCommand command, {
    String? projectId,
  }) async {
    final saved = command.copyWith(source: CommandSource.override);
    final index = _commands.indexWhere((item) => item.id == command.id);
    index < 0 ? _commands.add(saved) : _commands[index] = saved;
    return saved;
  }

  @override
  Future<List<VerificationGate>> loadGates(
    String taskId, {
    String? projectId,
  }) async => [...?_gates[taskId]];

  @override
  Future<List<VerificationGate>> saveGates(
    String taskId,
    List<VerificationGate> gates, {
    String? projectId,
  }) async {
    _gates[taskId] = [...gates];
    return [...gates];
  }

  @override
  Future<VerificationRun> startRun(
    String taskId,
    List<VerificationGate> gates, {
    Set<String>? gateIds,
  }) async {
    final selected = gates
        .where(
          (gate) =>
              gate.enabled && (gateIds == null || gateIds.contains(gate.id)),
        )
        .toList();
    final run = VerificationRun(
      id: 'verification-${++_nextId}',
      taskId: taskId,
      status: VerificationRunStatus.running,
      startedAt: DateTime.now(),
      gates: [
        for (final gate in selected)
          GateExecution(
            gateId: gate.id,
            label: gate.label,
            required: gate.required,
            status: GateStatus.queued,
          ),
      ],
    );
    _active[run.id] = run;
    scheduleMicrotask(
      () => _events.add(RunStarted(taskId: taskId, runId: run.id)),
    );
    if (autoComplete) scheduleMicrotask(() => _complete(run, selected));
    return run;
  }

  Future<void> _complete(
    VerificationRun run,
    List<VerificationGate> gates,
  ) async {
    var current = run;
    for (final gate in gates) {
      if (!_active.containsKey(run.id)) return;
      _events.add(
        GateStarted(taskId: run.taskId, runId: run.id, gateId: gate.id),
      );
      final result = _results[gate.id] ?? const ScriptedGateResult();
      if (result.stdout.isNotEmpty) {
        _events.add(
          GateOutput(
            taskId: run.taskId,
            runId: run.id,
            gateId: gate.id,
            text: result.stdout,
          ),
        );
      }
      if (result.stderr.isNotEmpty) {
        _events.add(
          GateOutput(
            taskId: run.taskId,
            runId: run.id,
            gateId: gate.id,
            text: result.stderr,
            stderr: true,
          ),
        );
      }
      final execution = GateExecution(
        gateId: gate.id,
        label: gate.label,
        required: gate.required,
        status: result.passed ? GateStatus.passed : GateStatus.failed,
        duration: result.duration,
        tests: result.tests,
        fileLinks: result.fileLinks,
        stdout: result.stdout,
        stderr: result.stderr,
      );
      current = current.copyWith(
        gates: current.gates
            .map((item) => item.gateId == gate.id ? execution : item)
            .toList(),
      );
      _events.add(
        GateFinished(taskId: run.taskId, runId: run.id, execution: execution),
      );
    }
    final failed = current.gates.any(
      (gate) => gate.status == GateStatus.failed,
    );
    current = current.copyWith(
      status: failed
          ? VerificationRunStatus.failed
          : VerificationRunStatus.passed,
      completedAt: DateTime.now(),
    );
    _active.remove(run.id);
    _history.putIfAbsent(run.taskId, () => []).insert(0, current);
    _events.add(RunFinished(taskId: run.taskId, runId: run.id, run: current));
  }

  @override
  Future<void> cancelRun(String runId) async {
    final run = _active.remove(runId);
    if (run == null) return;
    final cancelled = run.copyWith(
      status: VerificationRunStatus.cancelled,
      completedAt: DateTime.now(),
      gates: run.gates
          .map(
            (gate) =>
                gate.status == GateStatus.passed ||
                    gate.status == GateStatus.failed
                ? gate
                : gate.copyWith(status: GateStatus.cancelled),
          )
          .toList(),
    );
    _history.putIfAbsent(run.taskId, () => []).insert(0, cancelled);
    _events.add(RunFinished(taskId: run.taskId, runId: run.id, run: cancelled));
  }

  @override
  Future<VerificationRun> rerunRun(String runId) async {
    final previous = _history.values
        .expand((runs) => runs)
        .firstWhere((run) => run.id == runId);
    final failed = previous.gates
        .where((gate) => gate.status == GateStatus.failed)
        .map((gate) => gate.gateId)
        .toSet();
    return startRun(
      previous.taskId,
      _gates[previous.taskId] ?? const [],
      gateIds: failed,
    );
  }

  @override
  Future<List<VerificationRun>> listHistory(
    String taskId, {
    int limit = 20,
  }) async => [...?_history[taskId]].take(limit).toList();

  @override
  Future<VerificationCompletionReport?> loadReport(String runId) async {
    final run = _history.values
        .expand((runs) => runs)
        .where((run) => run.id == runId)
        .firstOrNull;
    return run == null
        ? null
        : VerificationCompletionReport(runId: run.id, status: run.status);
  }
}
