import 'dart:async';
import 'dart:convert';
import 'dart:io';

import '../core_client.dart';
import 'verification_models.dart';
import 'verification_repository.dart';

/// Minimal RPC and artifact surface used by [CoreVerificationRepository].
abstract interface class VerificationRpcClient {
  Stream<Map<String, dynamic>> get events;

  Future<Map<String, dynamic>> request(
    String method, {
    Map<String, dynamic> params = const {},
  });

  Future<String?> readArtifact(String hash, {required int maxBytes});
}

class CoreVerificationRpcClient implements VerificationRpcClient {
  CoreVerificationRpcClient(this._core);

  final CoreClient _core;

  @override
  Stream<Map<String, dynamic>> get events => _core.events;

  @override
  Future<Map<String, dynamic>> request(
    String method, {
    Map<String, dynamic> params = const {},
  }) => _core.request(method, params: params);

  @override
  Future<String?> readArtifact(String hash, {required int maxBytes}) async {
    if (!_artifactHash.hasMatch(hash) || maxBytes <= 0) return null;
    final separator = Platform.pathSeparator;
    final file = File(
      '${_core.dataDirectory.path}${separator}artifacts${separator}sha256'
      '$separator${hash.substring(0, 2)}$separator${hash.substring(2)}',
    );
    try {
      final handle = await file.open();
      try {
        final length = await handle.length();
        final bytes = await handle.read(maxBytes);
        final text = utf8.decode(bytes, allowMalformed: true);
        return length > bytes.length ? '$text\n… [artifact truncated]' : text;
      } finally {
        await handle.close();
      }
    } on FileSystemException {
      return null;
    }
  }
}

/// Maps the durable `verification.*` RPC family onto the desktop domain.
class CoreVerificationRepository implements VerificationRepository {
  CoreVerificationRepository(this._rpc, {this.maxArtifactBytes = 64 * 1024});

  factory CoreVerificationRepository.fromCore(CoreClient core) =>
      CoreVerificationRepository(CoreVerificationRpcClient(core));

  final VerificationRpcClient _rpc;
  final int maxArtifactBytes;

  @override
  late final Stream<VerificationEvent> events = _rpc.events
      .where(_isVerificationRefreshEvent)
      .asyncExpand(_decodeRefreshEvent)
      .asBroadcastStream();

  @override
  Future<List<ProjectCommand>> detectCommands({
    String? projectId,
    String? projectPath,
  }) async {
    if (projectId == null || projectId.isEmpty) return const [];
    return (await _loadCommands(projectId)).map(_decodeCommand).toList();
  }

  @override
  Future<ProjectCommand> saveCommandOverride(
    ProjectCommand command, {
    String? projectId,
  }) async {
    final resolvedProjectId = _requireProjectId(projectId);
    final commands = [...await _loadCommands(resolvedProjectId)];
    final index = commands.indexWhere(
      (item) => item['id']?.toString() == command.id,
    );
    if (index < 0) {
      commands.add(_encodeCommand(command));
    } else {
      commands[index] = {
        ...commands[index],
        'command': command.command,
        'cwd': command.cwd,
        'timeoutMs': command.timeout?.inMilliseconds,
      };
    }
    final saved = await _configure(resolvedProjectId, commands);
    final wire = saved.firstWhere(
      (item) => item['id']?.toString() == command.id,
      orElse: () =>
          saved.firstWhere((item) => item['key']?.toString() == command.label),
    );
    final decoded = _decodeCommand(wire);
    return decoded.copyWith(source: CommandSource.override);
  }

  @override
  Future<List<VerificationGate>> loadGates(
    String taskId, {
    String? projectId,
  }) async {
    if (projectId == null || projectId.isEmpty) return const [];
    return (await _loadCommands(projectId)).map(_decodeConfiguredGate).toList();
  }

  @override
  Future<List<VerificationGate>> saveGates(
    String taskId,
    List<VerificationGate> gates, {
    String? projectId,
  }) async {
    final resolvedProjectId = _requireProjectId(projectId);
    final existing = {
      for (final command in await _loadCommands(resolvedProjectId))
        command['id']?.toString(): command,
    };
    final configured = [
      for (final gate in gates)
        {
          ...?existing[gate.id],
          if (_uuid.hasMatch(gate.id)) 'id': gate.id,
          'key': existing[gate.id]?['key']?.toString() ?? gate.label,
          'kind': gate.kind,
          'command': gate.command,
          'cwd': gate.cwd,
          'required': gate.required,
          'enabled': gate.enabled,
          'timeoutMs': gate.timeout?.inMilliseconds,
        },
    ];
    return (await _configure(
      resolvedProjectId,
      configured,
    )).map(_decodeConfiguredGate).toList();
  }

  @override
  Future<VerificationRun> startRun(
    String taskId,
    List<VerificationGate> gates, {
    Set<String>? gateIds,
  }) async {
    final kinds = gateIds == null
        ? const <String>[]
        : gates
              .where((gate) => gateIds.contains(gate.id))
              .map((gate) => gate.kind)
              .toSet()
              .toList();
    final created = await _rpc.request(
      'verification.create',
      params: {'taskId': taskId, 'kinds': kinds},
    );
    final createdDetails = _map(created['verification']);
    final runId = _map(createdDetails['run'])['id']?.toString();
    if (runId == null || runId.isEmpty) {
      throw StateError('Core did not return the created verification run.');
    }
    final started = await _rpc.request(
      'verification.start',
      params: {'runId': runId},
    );
    return _decodeDetails(_map(started['verification']));
  }

  @override
  Future<void> cancelRun(String runId) async {
    await _rpc.request('verification.cancel', params: {'runId': runId});
  }

  @override
  Future<VerificationRun> rerunRun(String runId) async {
    final created = await _rpc.request(
      'verification.rerun',
      params: {'runId': runId},
    );
    final details = _map(created['verification']);
    final rerunId = _map(details['run'])['id']?.toString();
    if (rerunId == null || rerunId.isEmpty) {
      throw StateError('Core did not return the verification rerun.');
    }
    final started = await _rpc.request(
      'verification.start',
      params: {'runId': rerunId},
    );
    return _decodeDetails(_map(started['verification']));
  }

  @override
  Future<List<VerificationRun>> listHistory(
    String taskId, {
    int limit = 20,
  }) async {
    final response = await _rpc.request(
      'verification.list',
      params: {'taskId': taskId},
    );
    final summaries = _maps(response['verifications']).take(limit);
    return Future.wait(
      summaries.map((summary) async {
        final runId = summary['id']?.toString() ?? '';
        final values = await Future.wait([
          _rpc.request('verification.get', params: {'runId': runId}),
          _rpc.request('verification.history', params: {'runId': runId}),
        ]);
        final run = await _decodeDetails(_map(values[0]['verification']));
        final audit = _maps(
          values[1]['events'],
        ).map(_decodeAuditEntry).toList();
        return run.copyWith(auditTrail: audit);
      }),
    );
  }

  @override
  Future<VerificationCompletionReport?> loadReport(String runId) async {
    if (runId.isEmpty) return null;
    final response = await _rpc.request(
      'verification.report',
      params: {'runId': runId},
    );
    final report = _map(response['report']);
    if (report.isEmpty) return null;
    final approvals = _map(report['approvals']);
    final cost = _map(report['cost']);
    return VerificationCompletionReport(
      runId: report['runId']?.toString() ?? runId,
      status: _runStatus(report['status']),
      filesChanged: (report['filesChanged'] as List? ?? const [])
          .map((item) => item.toString())
          .toList(),
      approvalsTotal: _int(approvals['total']),
      approvalsApproved: _int(approvals['approved']),
      approvalsDenied: _int(approvals['denied']),
      estimatedCostMicros: _nullableInt(cost['estimatedMicros']),
      actualCostMicros: _nullableInt(cost['actualMicros']),
      currency: cost['currency']?.toString() ?? 'USD',
      limitations: (report['limitations'] as List? ?? const [])
          .map((item) => item.toString())
          .toList(),
    );
  }

  Future<List<Map<String, dynamic>>> _loadCommands(String projectId) async {
    final response = await _rpc.request(
      'verification.commands.list',
      params: {'projectId': projectId},
    );
    final commands = _maps(response['commands']);
    return commands;
  }

  Future<List<Map<String, dynamic>>> _configure(
    String projectId,
    List<Map<String, dynamic>> commands,
  ) async {
    final response = await _rpc.request(
      'verification.commands.configure',
      params: {
        'projectId': projectId,
        'commands': commands.map(_withoutNullValues).toList(),
      },
    );
    final saved = _maps(response['commands']);
    return saved;
  }

  Stream<VerificationEvent> _decodeRefreshEvent(
    Map<String, dynamic> wire,
  ) async* {
    final envelope = _eventEnvelope(wire);
    final payload = _map(envelope['payload']);
    final runId =
        payload['runId']?.toString() ??
        payload['verificationRunId']?.toString();
    if (runId == null || runId.isEmpty) return;
    try {
      final response = await _rpc.request(
        'verification.get',
        params: {'runId': runId},
      );
      final run = await _decodeDetails(_map(response['verification']));
      yield RunUpdated(taskId: run.taskId, runId: run.id, run: run);
    } on Object {
      // A later durable event will refresh the run if this event raced creation.
    }
  }

  Future<VerificationRun> _decodeDetails(Map<String, dynamic> details) async {
    final run = _map(details['run']);
    final results = _maps(details['results']);
    final artifacts = _maps(details['artifacts']);
    final output = <String, Map<String, String>>{};
    await Future.wait(
      artifacts.map((artifact) async {
        final gateId = artifact['gateId']?.toString();
        final kind = artifact['kind']?.toString();
        final hash = artifact['hash']?.toString();
        if (gateId == null ||
            (kind != 'stdout' && kind != 'stderr') ||
            hash == null) {
          return;
        }
        final content = await _rpc.readArtifact(
          hash,
          maxBytes: maxArtifactBytes,
        );
        if (content != null) {
          output.putIfAbsent(gateId, () => {})[kind!] = content;
        }
      }),
    );
    final gates = _maps(details['gates']).map((gate) {
      final wireGateId = gate['id']?.toString() ?? '';
      final gateResults = results
          .where((result) => result['gateId']?.toString() == wireGateId)
          .toList();
      final passed = gateResults
          .where((result) => result['status']?.toString() == 'passed')
          .length;
      final skipped = gateResults
          .where((result) => result['status']?.toString() == 'skipped')
          .length;
      final failed = gateResults.length - passed - skipped;
      final links = <VerificationFileLink>[];
      final seenLinks = <String>{};
      for (final result in gateResults) {
        final path = result['filePath']?.toString();
        if (path == null || path.isEmpty) continue;
        final line = _nullableInt(result['line']);
        if (seenLinks.add('$path:$line')) {
          links.add(VerificationFileLink(path: path, line: line));
        }
      }
      final startedAt = _date(gate['startedAt']);
      final completedAt = _date(gate['completedAt']);
      final resultDuration = gateResults.fold<int>(
        0,
        (sum, result) => sum + _int(result['durationMs']),
      );
      return GateExecution(
        gateId:
            gate['commandId']?.toString() ??
            gate['key']?.toString() ??
            wireGateId,
        label: gate['key']?.toString() ?? 'Verification gate',
        required: gate['required'] != false,
        status: _gateStatus(gate['status']),
        duration: startedAt != null && completedAt != null
            ? completedAt.difference(startedAt)
            : resultDuration > 0
            ? Duration(milliseconds: resultDuration)
            : null,
        tests: TestCounts(passed: passed, failed: failed, skipped: skipped),
        fileLinks: links,
        stdout: output[wireGateId]?['stdout'] ?? '',
        stderr: output[wireGateId]?['stderr'] ?? '',
      );
    }).toList();
    final createdAt = _date(run['createdAt']) ?? DateTime.now();
    return VerificationRun(
      id: run['id']?.toString() ?? '',
      taskId: run['taskId']?.toString() ?? '',
      status: _runStatus(run['status']),
      startedAt: _date(run['startedAt']) ?? createdAt,
      completedAt: _date(run['completedAt']),
      gates: gates,
    );
  }
}

final _artifactHash = RegExp(r'^[0-9a-f]{64}$');
final _uuid = RegExp(
  r'^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-5][0-9a-fA-F]{3}-'
  r'[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$',
);

String _requireProjectId(String? projectId) {
  if (projectId == null || projectId.isEmpty) {
    throw StateError('A project is required to configure verification gates.');
  }
  return projectId;
}

ProjectCommand _decodeCommand(Map<String, dynamic> json) => ProjectCommand(
  id: json['id']?.toString() ?? json['key']?.toString() ?? '',
  label: json['key']?.toString() ?? 'Verification command',
  command: json['command']?.toString() ?? '',
  kind: json['kind']?.toString() ?? 'custom',
  cwd: json['cwd']?.toString(),
  timeout: _durationMilliseconds(json['timeoutMs']),
);

VerificationGate _decodeConfiguredGate(Map<String, dynamic> json) =>
    VerificationGate(
      id: json['id']?.toString() ?? json['key']?.toString() ?? '',
      label: json['key']?.toString() ?? 'Verification gate',
      command: json['command']?.toString() ?? '',
      required: json['required'] != false,
      enabled: json['enabled'] != false,
      kind: json['kind']?.toString() ?? 'custom',
      cwd: json['cwd']?.toString(),
      timeout: _durationMilliseconds(json['timeoutMs']),
    );

Map<String, dynamic> _encodeCommand(ProjectCommand command) => {
  if (_uuid.hasMatch(command.id)) 'id': command.id,
  'key': command.label,
  'kind': command.kind,
  'command': command.command,
  'cwd': command.cwd,
  'required': true,
  'enabled': true,
  'timeoutMs': command.timeout?.inMilliseconds,
};

VerificationAuditEntry _decodeAuditEntry(Map<String, dynamic> json) =>
    VerificationAuditEntry(
      kind: json['kind']?.toString() ?? 'updated',
      actor: json['actor']?.toString() ?? 'system',
      createdAt: _date(json['createdAt']) ?? DateTime.now(),
    );

GateStatus _gateStatus(Object? value) => switch (value?.toString()) {
  'running' => GateStatus.running,
  'passed' => GateStatus.passed,
  'failed' || 'error' => GateStatus.failed,
  'cancelled' => GateStatus.cancelled,
  'skipped' => GateStatus.skipped,
  _ => GateStatus.queued,
};

VerificationRunStatus _runStatus(Object? value) => switch (value?.toString()) {
  'passed' => VerificationRunStatus.passed,
  'failed' || 'error' => VerificationRunStatus.failed,
  'cancelled' => VerificationRunStatus.cancelled,
  _ => VerificationRunStatus.running,
};

bool _isVerificationRefreshEvent(Map<String, dynamic> wire) {
  final kind = _eventEnvelope(wire)['kind']?.toString() ?? '';
  return kind.startsWith('verification.run.') ||
      kind == 'verification.gate.recorded' ||
      kind == 'task.changed';
}

Map<String, dynamic> _eventEnvelope(Map<String, dynamic> wire) {
  final nested = wire['event'];
  return nested is Map ? nested.cast<String, dynamic>() : wire;
}

Map<String, dynamic> _withoutNullValues(Map<String, dynamic> value) => {
  for (final entry in value.entries)
    if (entry.value != null) entry.key: entry.value,
};

Map<String, dynamic> _map(Object? value) =>
    value is Map ? value.cast<String, dynamic>() : <String, dynamic>{};

List<Map<String, dynamic>> _maps(Object? value) => (value as List? ?? const [])
    .whereType<Map>()
    .map((item) => item.cast<String, dynamic>())
    .toList();

int _int(Object? value) => (value as num?)?.toInt() ?? 0;
int? _nullableInt(Object? value) => (value as num?)?.toInt();
Duration? _durationMilliseconds(Object? value) {
  final milliseconds = _nullableInt(value);
  return milliseconds == null ? null : Duration(milliseconds: milliseconds);
}

DateTime? _date(Object? value) {
  final milliseconds = _nullableInt(value);
  return milliseconds == null
      ? null
      : DateTime.fromMillisecondsSinceEpoch(milliseconds);
}
