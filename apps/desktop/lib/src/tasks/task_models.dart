enum TaskStatus {
  backlog,
  planned,
  inProgress,
  paused,
  blocked,
  review,
  complete,
}

extension TaskStatusLabel on TaskStatus {
  String get label => switch (this) {
    TaskStatus.backlog => 'Backlog',
    TaskStatus.planned => 'Planned',
    TaskStatus.inProgress => 'In progress',
    TaskStatus.paused => 'Paused',
    TaskStatus.blocked => 'Blocked',
    TaskStatus.review => 'Review',
    TaskStatus.complete => 'Complete',
  };
}

enum PlanStepStatus { pending, inProgress, paused, blocked, complete }

extension PlanStepStatusLabel on PlanStepStatus {
  String get label => switch (this) {
    PlanStepStatus.pending => 'Pending',
    PlanStepStatus.inProgress => 'In progress',
    PlanStepStatus.paused => 'Paused',
    PlanStepStatus.blocked => 'Blocked',
    PlanStepStatus.complete => 'Complete',
  };
}

enum CriterionStatus { pending, passed, failed }

class PlanStep {
  const PlanStep({
    required this.id,
    required this.title,
    this.status = PlanStepStatus.pending,
    this.assignee,
    this.evidence = const [],
  });

  final String id;
  final String title;
  final PlanStepStatus status;
  final String? assignee;
  final List<String> evidence;

  PlanStep copyWith({
    String? title,
    PlanStepStatus? status,
    String? assignee,
    bool clearAssignee = false,
    List<String>? evidence,
  }) => PlanStep(
    id: id,
    title: title ?? this.title,
    status: status ?? this.status,
    assignee: clearAssignee ? null : assignee ?? this.assignee,
    evidence: evidence ?? this.evidence,
  );
}

class AcceptanceCriterion {
  const AcceptanceCriterion({
    required this.id,
    required this.title,
    this.required = true,
    this.status = CriterionStatus.pending,
    this.evidence = const [],
  });

  final String id;
  final String title;
  final bool required;
  final CriterionStatus status;
  final List<String> evidence;

  AcceptanceCriterion copyWith({
    String? title,
    bool? required,
    CriterionStatus? status,
    List<String>? evidence,
  }) => AcceptanceCriterion(
    id: id,
    title: title ?? this.title,
    required: required ?? this.required,
    status: status ?? this.status,
    evidence: evidence ?? this.evidence,
  );
}

class RoadmapTask {
  const RoadmapTask({
    required this.id,
    required this.title,
    required this.status,
    this.phase,
    this.assignee,
    this.planApproved = false,
    this.steps = const [],
    this.criteria = const [],
    this.tags = const [],
  });

  final String id;
  final String title;
  final TaskStatus status;
  final int? phase;
  final String? assignee;
  final bool planApproved;
  final List<PlanStep> steps;
  final List<AcceptanceCriterion> criteria;
  final List<String> tags;

  bool get acceptanceCriteriaMet => criteria
      .where((criterion) => criterion.required)
      .every((criterion) => criterion.status == CriterionStatus.passed);

  bool get stepsComplete =>
      steps.isNotEmpty &&
      steps.every((step) => step.status == PlanStepStatus.complete);

  bool get canComplete =>
      planApproved && stepsComplete && acceptanceCriteriaMet;

  RoadmapTask copyWith({
    String? title,
    TaskStatus? status,
    String? assignee,
    bool clearAssignee = false,
    bool? planApproved,
    List<PlanStep>? steps,
    List<AcceptanceCriterion>? criteria,
    List<String>? tags,
  }) => RoadmapTask(
    id: id,
    title: title ?? this.title,
    status: status ?? this.status,
    phase: phase,
    assignee: clearAssignee ? null : assignee ?? this.assignee,
    planApproved: planApproved ?? this.planApproved,
    steps: steps ?? this.steps,
    criteria: criteria ?? this.criteria,
    tags: tags ?? this.tags,
  );
}

enum TaskGrouping { status, phase, assignee, none }

extension TaskGroupingLabel on TaskGrouping {
  String get label => switch (this) {
    TaskGrouping.status => 'Status',
    TaskGrouping.phase => 'Phase',
    TaskGrouping.assignee => 'Assignee',
    TaskGrouping.none => 'None',
  };
}

class TaskSavedView {
  const TaskSavedView({
    required this.id,
    required this.name,
    this.query = '',
    this.statuses = const {},
    this.grouping = TaskGrouping.status,
  });

  final String id;
  final String name;
  final String query;
  final Set<TaskStatus> statuses;
  final TaskGrouping grouping;
}
