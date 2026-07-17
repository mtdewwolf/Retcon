/// RPC adapter for `terminal.*` methods on retcon-core.
abstract class TerminalService {
  Future<List<TerminalShell>> detectShells();
  Future<TerminalStartResult> start({
    required String shell,
    String? cwd,
    required int cols,
    required int rows,
  });
  Future<void> input({required int terminalId, required String data});
  Future<void> resize({
    required int terminalId,
    required int cols,
    required int rows,
  });
  Future<void> kill({required int terminalId});
  Future<List<TerminalSessionSummary>> listRecent({int limit = 20});
  Future<String> scrollback({required String sessionId});
}

class TerminalShell {
  const TerminalShell({required this.id, required this.path});
  final String id;
  final String path;

  factory TerminalShell.fromJson(Map<String, dynamic> json) => TerminalShell(
    id: json['id']?.toString() ?? '',
    path: json['path']?.toString() ?? '',
  );
}

class TerminalStartResult {
  const TerminalStartResult({
    required this.terminalId,
    required this.sessionId,
    required this.shell,
  });
  final int terminalId;
  final String sessionId;
  final String shell;

  factory TerminalStartResult.fromJson(Map<String, dynamic> json) =>
      TerminalStartResult(
        terminalId: (json['terminalId'] as num?)?.toInt() ?? 0,
        sessionId: json['sessionId']?.toString() ?? '',
        shell: json['shell']?.toString() ?? '',
      );
}

class TerminalSessionSummary {
  const TerminalSessionSummary({
    required this.sessionId,
    required this.status,
    required this.shell,
    required this.cwd,
    this.startedAt,
    this.endedAt,
    this.logArtifactHash,
  });
  final String sessionId;
  final String status;
  final String shell;
  final String cwd;
  final int? startedAt;
  final int? endedAt;
  final String? logArtifactHash;

  factory TerminalSessionSummary.fromJson(Map<String, dynamic> json) =>
      TerminalSessionSummary(
        sessionId: json['sessionId']?.toString() ?? '',
        status: json['status']?.toString() ?? '',
        shell: json['shell']?.toString() ?? '',
        cwd: json['cwd']?.toString() ?? '',
        startedAt: (json['startedAt'] as num?)?.toInt(),
        endedAt: (json['endedAt'] as num?)?.toInt(),
        logArtifactHash: json['logArtifactHash']?.toString(),
      );
}

typedef TerminalRequest = Future<Map<String, dynamic>> Function(
  String method, {
  Map<String, dynamic> params,
});

class RpcTerminalService implements TerminalService {
  RpcTerminalService(this._request);
  final TerminalRequest _request;

  @override
  Future<List<TerminalShell>> detectShells() async {
    final result = await _request('terminal.detectShells');
    return (result['shells'] as List? ?? const [])
        .map(
          (item) => TerminalShell.fromJson(
            (item as Map?)?.cast<String, dynamic>() ?? const {},
          ),
        )
        .toList();
  }

  @override
  Future<TerminalStartResult> start({
    required String shell,
    String? cwd,
    required int cols,
    required int rows,
  }) async {
    final result = await _request(
      'terminal.start',
      params: {
        'shell': shell,
        if (cwd != null) 'cwd': cwd,
        'cols': cols,
        'rows': rows,
      },
    );
    return TerminalStartResult.fromJson(result);
  }

  @override
  Future<void> input({required int terminalId, required String data}) =>
      _request('terminal.input', params: {'id': terminalId, 'data': data});

  @override
  Future<void> resize({
    required int terminalId,
    required int cols,
    required int rows,
  }) =>
      _request(
        'terminal.resize',
        params: {'id': terminalId, 'cols': cols, 'rows': rows},
      );

  @override
  Future<void> kill({required int terminalId}) =>
      _request('terminal.kill', params: {'id': terminalId});

  @override
  Future<List<TerminalSessionSummary>> listRecent({int limit = 20}) async {
    final result = await _request('terminal.list', params: {'limit': limit});
    return (result['sessions'] as List? ?? const [])
        .map(
          (item) => TerminalSessionSummary.fromJson(
            (item as Map?)?.cast<String, dynamic>() ?? const {},
          ),
        )
        .toList();
  }

  @override
  Future<String> scrollback({required String sessionId}) async {
    final result = await _request(
      'terminal.scrollback',
      params: {'sessionId': sessionId},
    );
    return result['text']?.toString() ?? '';
  }
}
