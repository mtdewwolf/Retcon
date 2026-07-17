import 'dart:convert';

import '../core_client.dart';
import 'task_models.dart';
import 'task_repository.dart';

/// Minimal RPC/event surface used by [CoreTaskRepository].
///
/// Keeping this separate from [CoreClient] makes adapter tests deterministic
/// and keeps wire details out of the task widgets and controller.
abstract interface class TaskRpcClient {
  Stream<Map<String, dynamic>> get events;

  Future<Map<String, dynamic>> request(
    String method, {
    Map<String, dynamic> params = const {},
  });
}

class CoreTaskRpcClient implements TaskRpcClient {
  CoreTaskRpcClient(this._core);

  final CoreClient _core;

  @override
  Stream<Map<String, dynamic>> get events => _core.events;

  @override
  Future<Map<String, dynamic>> request(
    String method, {
    Map<String, dynamic> params = const {},
  }) => _core.request(method, params: params);
}

/// Maps the Phase 21 desktop domain onto the durable `task.*` RPC family.
class CoreTaskRepository implements TaskRepository {
  CoreTaskRepository(this._rpc);

  factory CoreTaskRepository.fromCore(CoreClient core) =>
      CoreTaskRepository(CoreTaskRpcClient(core));

  final TaskRpcClient _rpc;
  final List<TaskSavedView> _savedViews = [];

  @override
  Stream<void> get changes =>
      _rpc.events.where(_isTaskChanged).map<void>((_) {});

  @override
  Future<List<RoadmapTask>> listTasks({String? projectId}) async {
    final result = await _rpc.request(
      'task.list',
      params: {'projectId': ?projectId},
    );
    final summaries = _maps(result['tasks']);
    final details = await Future.wait(
      summaries.map((summary) async {
        final taskId = summary['id']?.toString();
        if (taskId == null || taskId.isEmpty) return null;
        return _loadTask(taskId);
      }),
    );
    return details.whereType<RoadmapTask>().toList();
  }

  @override
  Future<RoadmapTask> createTask(String title, {String? projectId}) async {
    final trimmed = title.trim();
    if (trimmed.isEmpty) {
      throw ArgumentError('Task title cannot be empty.');
    }
    final result = await _rpc.request(
      'task.create',
      params: {
        'title': trimmed,
        'projectId': ?projectId,
      },
    );
    return _decodeTask(_map(result['task']));
  }

  @override
  Future<RoadmapTask> saveTask(RoadmapTask task, {String? projectId}) async {
    final before = await _getDetails(task.id);
    final persisted = _decodeTask(before);
    final taskRecord = _map(before['task']);
    await _rpc.request(
      'task.update',
      params: {
        'taskId': task.id,
        'title': task.title,
        'agent': task.assignee,
        'projectId': ?projectId,
        'description': _encodeTaskDescription(task, taskRecord['description']),
      },
    );

    await _replacePlan(task, before);
    await _syncCriteria(task, before);

    if (_taskStatusToWire(task.status) != _taskStatusToWire(persisted.status)) {
      await _rpc.request(
        'task.status.set',
        params: {'taskId': task.id, 'status': _taskStatusToWire(task.status)},
      );
    }
    return _loadTask(task.id);
  }

  @override
  Future<List<TaskSavedView>> listSavedViews({String? projectId}) async => [
    ..._savedViews,
  ];

  @override
  Future<TaskSavedView> saveView(
    TaskSavedView view, {
    String? projectId,
  }) async {
    final index = _savedViews.indexWhere((item) => item.id == view.id);
    if (index < 0) {
      _savedViews.add(view);
    } else {
      _savedViews[index] = view;
    }
    return view;
  }

  Future<RoadmapTask> _loadTask(String taskId) async =>
      _decodeTask(await _getDetails(taskId));

  Future<Map<String, dynamic>> _getDetails(String taskId) async {
    final result = await _rpc.request('task.get', params: {'taskId': taskId});
    return _map(result['task']);
  }

  Future<void> _replacePlan(
    RoadmapTask task,
    Map<String, dynamic> details,
  ) async {
    final existing = {
      for (final step in _maps(details['steps'])) step['id']?.toString(): step,
    };
    await _rpc.request(
      'task.plan.replace',
      params: {
        'taskId': task.id,
        'steps': [
          for (final step in task.steps)
            {
              if (_isUuid(step.id)) 'id': step.id,
              'title': step.title,
              'status': _stepStatusToWire(step.status),
              'description': _encodeStepDescription(
                step,
                existing[step.id]?['description'],
              ),
              'dependencyIds': const <String>[],
            },
        ],
      },
    );
  }

  Future<void> _syncCriteria(
    RoadmapTask task,
    Map<String, dynamic> details,
  ) async {
    final existing = {
      for (final criterion in _maps(details['acceptanceCriteria']))
        criterion['id']?.toString(): criterion,
    };
    final retained = <String>{};

    for (var index = 0; index < task.criteria.length; index++) {
      final desired = task.criteria[index];
      var current = existing[desired.id];
      var criterionId = desired.id;
      if (current == null) {
        final created = await _rpc.request(
          'task.acceptance.create',
          params: {
            'taskId': task.id,
            if (_isUuid(desired.id)) 'criterionId': desired.id,
            'description': desired.title,
            'sortOrder': index,
            'required': desired.required,
          },
        );
        current = _map(created['acceptanceCriterion']);
        criterionId = current['id']?.toString() ?? desired.id;
      } else if (current['description']?.toString() != desired.title ||
          current['isRequired'] != desired.required ||
          (current['sortOrder'] as num?)?.toInt() != index) {
        await _rpc.request(
          'task.acceptance.update',
          params: {
            'criterionId': criterionId,
            'description': desired.title,
            'sortOrder': index,
            'required': desired.required,
          },
        );
      }
      retained.add(criterionId);

      final currentEvidence = _evidenceStrings(current['evidence']);
      for (final evidence in desired.evidence) {
        if (currentEvidence.contains(evidence)) continue;
        await _rpc.request(
          'task.acceptance.evidence',
          params: {'criterionId': criterionId, 'evidence': evidence},
        );
      }

      final currentStatus = _criterionStatus(current['status']);
      if (desired.status != CriterionStatus.pending &&
          desired.status != currentStatus) {
        await _rpc.request(
          'task.acceptance.evaluate',
          params: {
            'criterionId': criterionId,
            'passed': desired.status == CriterionStatus.passed,
          },
        );
      }
    }

    for (final entry in existing.entries) {
      if (entry.key == null || retained.contains(entry.key)) continue;
      await _rpc.request(
        'task.acceptance.delete',
        params: {'criterionId': entry.key},
      );
    }
  }

  RoadmapTask _decodeTask(Map<String, dynamic> details) {
    final task = _map(details['task']);
    final metadata = _decodeMetadata(task['description']);
    return RoadmapTask(
      id: task['id']?.toString() ?? '',
      title: task['title']?.toString() ?? 'Untitled task',
      status: _taskStatus(task['status']),
      phase: (metadata.desktop['phase'] as num?)?.toInt(),
      assignee: task['agent']?.toString(),
      planApproved: metadata.desktop['planApproved'] == true,
      tags: (metadata.desktop['tags'] as List? ?? const [])
          .map((tag) => tag.toString())
          .toList(),
      steps: _maps(details['steps']).map(_decodeStep).toList(),
      criteria: _maps(
        details['acceptanceCriteria'],
      ).map(_decodeCriterion).toList(),
    );
  }

  PlanStep _decodeStep(Map<String, dynamic> json) {
    final metadata = _decodeMetadata(json['description']);
    return PlanStep(
      id: json['id']?.toString() ?? '',
      title: json['title']?.toString() ?? 'Untitled step',
      status: _stepStatus(json['status']),
      assignee: metadata.desktop['assignee']?.toString(),
      evidence: (metadata.desktop['evidence'] as List? ?? const [])
          .map((item) => item.toString())
          .toList(),
    );
  }

  AcceptanceCriterion _decodeCriterion(Map<String, dynamic> json) =>
      AcceptanceCriterion(
        id: json['id']?.toString() ?? '',
        title: json['description']?.toString() ?? 'Acceptance criterion',
        required: json['isRequired'] != false,
        status: _criterionStatus(json['status']),
        evidence: _evidenceStrings(json['evidence']),
      );
}

bool _isTaskChanged(Map<String, dynamic> event) {
  final nested = event['event'];
  final envelope = nested is Map ? nested.cast<String, dynamic>() : event;
  return envelope['kind']?.toString() == 'task.changed' ||
      envelope['name']?.toString() == 'task.changed' ||
      envelope['type']?.toString() == 'task.changed';
}

Map<String, dynamic> _map(Object? value) =>
    value is Map ? value.cast<String, dynamic>() : <String, dynamic>{};

List<Map<String, dynamic>> _maps(Object? value) => (value as List? ?? const [])
    .whereType<Map>()
    .map((item) => item.cast<String, dynamic>())
    .toList();

class _Metadata {
  const _Metadata(this.originalDescription, this.desktop);
  final String? originalDescription;
  final Map<String, dynamic> desktop;
}

_Metadata _decodeMetadata(Object? rawValue) {
  final raw = rawValue?.toString();
  if (raw == null || raw.isEmpty) return const _Metadata(null, {});
  try {
    final value = jsonDecode(raw);
    if (value is Map && value['_retconDesktop'] is Map) {
      return _Metadata(
        value['description']?.toString(),
        (value['_retconDesktop'] as Map).cast<String, dynamic>(),
      );
    }
  } on FormatException {
    // Preserve user-authored descriptions that are not desktop metadata.
  }
  return _Metadata(raw, const {});
}

String _encodeTaskDescription(RoadmapTask task, Object? current) {
  final metadata = _decodeMetadata(current);
  return jsonEncode({
    if (metadata.originalDescription != null)
      'description': metadata.originalDescription,
    '_retconDesktop': {
      if (task.phase != null) 'phase': task.phase,
      'planApproved': task.planApproved,
      'tags': task.tags,
    },
  });
}

String _encodeStepDescription(PlanStep step, Object? current) {
  final metadata = _decodeMetadata(current);
  return jsonEncode({
    if (metadata.originalDescription != null)
      'description': metadata.originalDescription,
    '_retconDesktop': {
      if (step.assignee != null) 'assignee': step.assignee,
      'evidence': step.evidence,
    },
  });
}

TaskStatus _taskStatus(Object? value) => switch (value?.toString()) {
  'planned' || 'ready' => TaskStatus.planned,
  'in_progress' => TaskStatus.inProgress,
  'pending' => TaskStatus.paused,
  'blocked' || 'failed' || 'cancelled' => TaskStatus.blocked,
  'review' => TaskStatus.review,
  'completed' || 'done' => TaskStatus.complete,
  _ => TaskStatus.backlog,
};

String _taskStatusToWire(TaskStatus value) => switch (value) {
  TaskStatus.backlog => 'backlog',
  TaskStatus.planned => 'planned',
  TaskStatus.inProgress => 'in_progress',
  TaskStatus.paused => 'pending',
  TaskStatus.blocked => 'blocked',
  TaskStatus.review => 'review',
  TaskStatus.complete => 'completed',
};

PlanStepStatus _stepStatus(Object? value) => switch (value?.toString()) {
  'in_progress' => PlanStepStatus.inProgress,
  'blocked' => PlanStepStatus.blocked,
  'completed' || 'skipped' => PlanStepStatus.complete,
  _ => PlanStepStatus.pending,
};

String _stepStatusToWire(PlanStepStatus value) => switch (value) {
  PlanStepStatus.pending || PlanStepStatus.paused => 'pending',
  PlanStepStatus.inProgress => 'in_progress',
  PlanStepStatus.blocked => 'blocked',
  PlanStepStatus.complete => 'completed',
};

CriterionStatus _criterionStatus(Object? value) => switch (value?.toString()) {
  'passed' || 'overridden' => CriterionStatus.passed,
  'failed' => CriterionStatus.failed,
  _ => CriterionStatus.pending,
};

List<String> _evidenceStrings(Object? value) => (value as List? ?? const [])
    .map((item) => item is String ? item : jsonEncode(item))
    .toList();

bool _isUuid(String value) => RegExp(
  r'^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-5][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$',
).hasMatch(value);
