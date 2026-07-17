enum CommandSource { detected, override }

class ProjectCommand {
  const ProjectCommand({
    required this.id,
    required this.label,
    required this.command,
    this.source = CommandSource.detected,
    this.kind = 'custom',
    this.cwd,
    this.timeout,
  });

  final String id;
  final String label;
  final String command;
  final CommandSource source;
  final String kind;
  final String? cwd;
  final Duration? timeout;

  ProjectCommand copyWith({
    String? label,
    String? command,
    CommandSource? source,
    String? kind,
    String? cwd,
    Duration? timeout,
  }) => ProjectCommand(
    id: id,
    label: label ?? this.label,
    command: command ?? this.command,
    source: source ?? this.source,
    kind: kind ?? this.kind,
    cwd: cwd ?? this.cwd,
    timeout: timeout ?? this.timeout,
  );
}

class VerificationGate {
  const VerificationGate({
    required this.id,
    required this.label,
    required this.command,
    this.required = true,
    this.enabled = true,
    this.kind = 'custom',
    this.cwd,
    this.timeout,
  });

  final String id;
  final String label;
  final String command;
  final bool required;
  final bool enabled;
  final String kind;
  final String? cwd;
  final Duration? timeout;

  VerificationGate copyWith({
    String? label,
    String? command,
    bool? required,
    bool? enabled,
    String? kind,
    String? cwd,
    Duration? timeout,
  }) => VerificationGate(
    id: id,
    label: label ?? this.label,
    command: command ?? this.command,
    required: required ?? this.required,
    enabled: enabled ?? this.enabled,
    kind: kind ?? this.kind,
    cwd: cwd ?? this.cwd,
    timeout: timeout ?? this.timeout,
  );
}

enum GateStatus { queued, running, passed, failed, cancelled, skipped }

enum VerificationRunStatus { running, passed, failed, cancelled }

class TestCounts {
  const TestCounts({this.passed = 0, this.failed = 0, this.skipped = 0});

  final int passed;
  final int failed;
  final int skipped;
  int get total => passed + failed + skipped;
}

class VerificationFileLink {
  const VerificationFileLink({required this.path, this.line});
  final String path;
  final int? line;
}

class GateExecution {
  const GateExecution({
    required this.gateId,
    required this.label,
    required this.required,
    required this.status,
    this.duration,
    this.tests = const TestCounts(),
    this.fileLinks = const [],
    this.stdout = '',
    this.stderr = '',
  });

  final String gateId;
  final String label;
  final bool required;
  final GateStatus status;
  final Duration? duration;
  final TestCounts tests;
  final List<VerificationFileLink> fileLinks;
  final String stdout;
  final String stderr;

  GateExecution copyWith({
    GateStatus? status,
    Duration? duration,
    TestCounts? tests,
    List<VerificationFileLink>? fileLinks,
    String? stdout,
    String? stderr,
  }) => GateExecution(
    gateId: gateId,
    label: label,
    required: required,
    status: status ?? this.status,
    duration: duration ?? this.duration,
    tests: tests ?? this.tests,
    fileLinks: fileLinks ?? this.fileLinks,
    stdout: stdout ?? this.stdout,
    stderr: stderr ?? this.stderr,
  );
}

class VerificationRun {
  const VerificationRun({
    required this.id,
    required this.taskId,
    required this.status,
    required this.startedAt,
    this.completedAt,
    this.gates = const [],
    this.auditTrail = const [],
  });

  final String id;
  final String taskId;
  final VerificationRunStatus status;
  final DateTime startedAt;
  final DateTime? completedAt;
  final List<GateExecution> gates;
  final List<VerificationAuditEntry> auditTrail;

  Duration get duration =>
      (completedAt ?? DateTime.now()).difference(startedAt);
  int get passedTests => gates.fold(0, (sum, gate) => sum + gate.tests.passed);
  int get failedTests => gates.fold(0, (sum, gate) => sum + gate.tests.failed);
  bool get hasRequiredFailures =>
      gates.any((gate) => gate.required && gate.status != GateStatus.passed);

  VerificationRun copyWith({
    VerificationRunStatus? status,
    DateTime? completedAt,
    List<GateExecution>? gates,
    List<VerificationAuditEntry>? auditTrail,
  }) => VerificationRun(
    id: id,
    taskId: taskId,
    status: status ?? this.status,
    startedAt: startedAt,
    completedAt: completedAt ?? this.completedAt,
    gates: gates ?? this.gates,
    auditTrail: auditTrail ?? this.auditTrail,
  );
}

class VerificationAuditEntry {
  const VerificationAuditEntry({
    required this.kind,
    required this.actor,
    required this.createdAt,
  });

  final String kind;
  final String actor;
  final DateTime createdAt;
}

class VerificationCompletionReport {
  const VerificationCompletionReport({
    required this.runId,
    required this.status,
    this.filesChanged = const [],
    this.approvalsTotal = 0,
    this.approvalsApproved = 0,
    this.approvalsDenied = 0,
    this.estimatedCostMicros,
    this.actualCostMicros,
    this.currency = 'USD',
    this.limitations = const [],
  });

  final String runId;
  final VerificationRunStatus status;
  final List<String> filesChanged;
  final int approvalsTotal;
  final int approvalsApproved;
  final int approvalsDenied;
  final int? estimatedCostMicros;
  final int? actualCostMicros;
  final String currency;
  final List<String> limitations;
}

class VerificationComparison {
  const VerificationComparison({required this.current, required this.previous});
  final VerificationRun current;
  final VerificationRun previous;
  int get passedTestDelta => current.passedTests - previous.passedTests;
  int get failedTestDelta => current.failedTests - previous.failedTests;
  Duration get durationDelta => current.duration - previous.duration;
}

sealed class VerificationEvent {
  const VerificationEvent({required this.taskId, required this.runId});
  final String taskId;
  final String runId;
}

class RunStarted extends VerificationEvent {
  const RunStarted({required super.taskId, required super.runId});
}

class GateStarted extends VerificationEvent {
  const GateStarted({
    required super.taskId,
    required super.runId,
    required this.gateId,
  });
  final String gateId;
}

class GateOutput extends VerificationEvent {
  const GateOutput({
    required super.taskId,
    required super.runId,
    required this.gateId,
    required this.text,
    this.stderr = false,
  });
  final String gateId;
  final String text;
  final bool stderr;
}

class GateFinished extends VerificationEvent {
  const GateFinished({
    required super.taskId,
    required super.runId,
    required this.execution,
  });
  final GateExecution execution;
}

class RunFinished extends VerificationEvent {
  const RunFinished({
    required super.taskId,
    required super.runId,
    required this.run,
  });
  final VerificationRun run;
}

class RunUpdated extends VerificationEvent {
  const RunUpdated({
    required super.taskId,
    required super.runId,
    required this.run,
  });
  final VerificationRun run;
}
