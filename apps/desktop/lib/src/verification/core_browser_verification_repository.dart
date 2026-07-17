import 'dart:async';
import 'dart:io';

import '../core_client.dart';
import 'browser_verification_models.dart';
import 'browser_verification_repository.dart';

/// Narrow provisional transport surface. The Phase 25 generated RPC types can
/// replace this adapter without leaking wire DTOs into widgets or controllers.
abstract interface class BrowserVerificationRpcClient {
  Stream<Map<String, dynamic>> get events;

  Future<Map<String, dynamic>> request(
    String method, {
    Map<String, dynamic> params = const {},
  });

  String? artifactPath(String hash);
}

class CoreBrowserVerificationRpcClient implements BrowserVerificationRpcClient {
  CoreBrowserVerificationRpcClient(this._core);

  final CoreClient _core;

  @override
  Stream<Map<String, dynamic>> get events => _core.events;

  @override
  Future<Map<String, dynamic>> request(
    String method, {
    Map<String, dynamic> params = const {},
  }) => _core.request(method, params: params);

  @override
  String? artifactPath(String hash) {
    if (!_artifactHash.hasMatch(hash)) return null;
    final separator = Platform.pathSeparator;
    return '${_core.dataDirectory.path}${separator}artifacts${separator}sha256'
        '$separator${hash.substring(0, 2)}$separator${hash.substring(2)}';
  }
}

class CoreBrowserVerificationRepository
    implements BrowserVerificationRepository {
  CoreBrowserVerificationRepository(
    this._rpc, {
    required this.projectId,
    BrowserVerificationDtoCodec codec = const BrowserVerificationDtoCodec(),
  }) : _codec = codec;

  factory CoreBrowserVerificationRepository.fromCore(
    CoreClient core, {
    required String projectId,
  }) => CoreBrowserVerificationRepository(
    CoreBrowserVerificationRpcClient(core),
    projectId: projectId,
  );

  final BrowserVerificationRpcClient _rpc;
  final BrowserVerificationDtoCodec _codec;
  final String projectId;
  final Set<String> _knownDefinitionIds = {};

  @override
  late final Stream<BrowserVerificationEvent> events = _rpc.events
      .where(_isBrowserVerificationEvent)
      .asyncExpand(_refreshEvent)
      .asBroadcastStream();

  @override
  Future<BrowserVerificationDefinition?> loadDefinition(String taskId) async {
    final response = await _rpc.request(
      'browser.verification.definition.list',
      params: {'projectId': projectId},
    );
    final definitions = _maps(response['definitions']);
    _knownDefinitionIds.addAll(
      definitions.map((item) => item['id']?.toString() ?? ''),
    );
    final matches = definitions.where(
      (item) =>
          item['taskId']?.toString() == taskId &&
          item['status']?.toString() != 'archived',
    );
    return matches.isEmpty ? null : _codec.decodeDefinition(matches.first);
  }

  @override
  Future<BrowserVerificationDefinition> saveDefinition(
    BrowserVerificationDefinition definition,
  ) async {
    final update = _knownDefinitionIds.contains(definition.id);
    final response = await _rpc.request(
      update
          ? 'browser.verification.definition.update'
          : 'browser.verification.definition.create',
      params: _withoutNullValues({
        'projectId': projectId,
        if (update || _uuid.hasMatch(definition.id))
          'definitionId': definition.id,
        'taskId': definition.taskId,
        ..._codec.encodeDefinition(definition),
      }),
    );
    final saved = _codec.decodeDefinition(_map(response['definition']));
    _knownDefinitionIds.add(saved.id);
    return saved;
  }

  @override
  Future<BrowserVerificationRun> startRun(
    BrowserVerificationDefinition definition, {
    String? devServerInstanceId,
  }) async {
    if (devServerInstanceId == null || devServerInstanceId.isEmpty) {
      throw StateError(
        'A running development server is required for browser verification.',
      );
    }
    final response = await _rpc.request(
      'browser.verification.run',
      params: {
        'taskId': definition.taskId,
        'definitionId': definition.id,
        'devServerInstanceId': devServerInstanceId,
      },
    );
    return _decodeRun(_map(response['verification']));
  }

  @override
  Future<void> cancelRun(String runId) async {
    await _rpc.request('browser.verification.cancel', params: {'runId': runId});
  }

  @override
  Future<BrowserVerificationRun> reviewRun(
    String runId, {
    required bool approve,
    String reason = '',
  }) async {
    final response = await _rpc.request(
      'browser.verification.review',
      params: {
        'runId': runId,
        'decision': approve ? 'approve' : 'reject',
        'reason': reason,
      },
    );
    return _decodeRun(_map(response['verification']));
  }

  @override
  Future<List<BrowserVerificationRun>> listHistory(
    String taskId, {
    int limit = 20,
  }) async {
    final response = await _rpc.request(
      'browser.verification.list',
      params: {'taskId': taskId},
    );
    return Future.wait(
      _maps(response['runs']).take(limit).map((summary) async {
        final details = await _rpc.request(
          'browser.verification.get',
          params: {'runId': summary['id']?.toString() ?? ''},
        );
        return _decodeRun(_map(details['verification']));
      }),
    );
  }

  @override
  Future<BrowserVisualComparison> approveBaseline(
    String runId,
    String comparisonId,
  ) async {
    await _rpc.request(
      'browser.verification.baseline.approve',
      params: {'comparisonId': comparisonId},
    );
    final refreshed = await _rpc.request(
      'browser.verification.get',
      params: {'runId': runId},
    );
    return _decodeRun(
      _map(refreshed['verification']),
    ).visualComparisons.firstWhere((item) => item.id == comparisonId);
  }

  BrowserVerificationRun _decodeRun(Map<String, dynamic> wire) =>
      _codec.decodeRun(wire, artifactPath: _rpc.artifactPath);

  Stream<BrowserVerificationEvent> _refreshEvent(
    Map<String, dynamic> wire,
  ) async* {
    final envelope = _eventEnvelope(wire);
    final payload = _map(envelope['payload']);
    final runId = payload['runId']?.toString();
    if (runId == null || runId.isEmpty) return;
    try {
      final response = await _rpc.request(
        'browser.verification.get',
        params: {'runId': runId},
      );
      final run = _decodeRun(_map(response['verification']));
      yield BrowserVerificationRunUpdated(
        taskId: run.taskId,
        runId: run.id,
        run: run,
      );
    } on Object {
      // A later durable event will reconcile state if this raced persistence.
    }
  }
}

/// All provisional Phase 25 wire assumptions are intentionally isolated here.
class BrowserVerificationDtoCodec {
  const BrowserVerificationDtoCodec();

  BrowserVerificationDefinition decodeDefinition(Map<String, dynamic> wire) =>
      BrowserVerificationDefinition(
        id: wire['id']?.toString() ?? '',
        taskId: wire['taskId']?.toString() ?? '',
        name: wire['name']?.toString() ?? 'Browser verification',
        targetUrl: wire['targetUrl']?.toString() ?? '',
        requiredServerId:
            wire['devServerConfigId']?.toString() ??
            wire['serverRef']?.toString() ??
            wire['requiredServerId']?.toString(),
        viewports: _maps(
          wire['variants'] ?? wire['viewports'],
        ).map(decodeViewport).toList(),
        timeout: Duration(milliseconds: _int(wire['timeoutMs'], 30000)),
        retryCount: _int(
          wire['maxRetries'] ?? wire['retries'] ?? wire['retryCount'],
          1,
        ),
        visualThreshold: _ratio(
          _map(wire['visualPolicy'])['maxDiffPixelRatio'] ??
              wire['visualThreshold'],
          fallback: 0.01,
        ),
        maskSelectors: _strings(_map(wire['visualPolicy'])['maskSelectors']),
        ignoreSelectors: _strings(
          _map(wire['visualPolicy'])['ignoreSelectors'],
        ),
        steps: _decodeDefinitionSteps(wire),
        required: wire['required'] != false,
        failOnAccessibility: wire['failOnAccessibility'] != false,
        enabled:
            wire['status']?.toString() != 'disabled' &&
            wire['status']?.toString() != 'archived',
      );

  Map<String, dynamic> encodeDefinition(BrowserVerificationDefinition value) =>
      {
        'name': value.name,
        'targetUrl': value.targetUrl,
        'devServerConfigId': value.requiredServerId,
        'variants': value.viewports.map(encodeViewport).toList(),
        'timeoutMs': value.timeout.inMilliseconds,
        'maxRetries': value.retryCount,
        'status': value.enabled ? 'active' : 'disabled',
        'failOnAccessibility': value.failOnAccessibility,
        'visualPolicy': {
          'maxDiffPixelRatio': value.visualThreshold,
          'perceptualThreshold': value.visualThreshold,
          'maskSelectors': value.maskSelectors,
          'ignoreSelectors': value.ignoreSelectors,
        },
        'steps': value.steps.map(encodeStep).toList(),
        'assertions': [
          for (final step in value.steps)
            for (final assertion in step.assertions)
              {...encodeAssertion(assertion), 'stepId': step.id},
        ],
        'required': value.required,
      };

  BrowserViewport decodeViewport(Map<String, dynamic> wire) => BrowserViewport(
    id: wire['key']?.toString() ?? wire['id']?.toString() ?? '',
    label:
        wire['label']?.toString() ??
        _title(wire['key']?.toString() ?? 'Viewport'),
    width: _int(wire['width'], 1440),
    height: _int(wire['height'], 900),
    deviceScaleFactor: _double(
      wire['deviceScaleFactor'] ?? wire['deviceScale'],
      1,
    ),
  );

  Map<String, dynamic> encodeViewport(BrowserViewport value) => {
    'name': value.id,
    'width': value.width,
    'height': value.height,
    'deviceScaleFactor': value.deviceScaleFactor,
  };

  BrowserVerificationStep decodeStep(Map<String, dynamic> wire) =>
      BrowserVerificationStep(
        id: wire['id']?.toString() ?? '',
        label: wire['label']?.toString() ?? 'Browser step',
        kind: _stepKind(wire['kind']),
        target:
            wire['target']?.toString() ??
            wire['url']?.toString() ??
            wire['selector']?.toString() ??
            wire['key']?.toString(),
        value:
            wire['value']?.toString() ??
            wire['text']?.toString() ??
            wire['milliseconds']?.toString(),
        timeout: Duration(milliseconds: _int(wire['timeoutMs'], 10000)),
        assertions: _maps(wire['assertions']).map(decodeAssertion).toList(),
        enabled: wire['enabled'] != false,
      );

  Map<String, dynamic> encodeStep(BrowserVerificationStep value) => {
    'id': value.id,
    'label': value.label,
    'kind': value.kind == BrowserStepKind.waitFor ? 'wait' : value.kind.name,
    'target': value.target,
    'value': value.value,
    'timeoutMs': value.timeout.inMilliseconds,
    'enabled': value.enabled,
    'assertions': value.assertions.map(encodeAssertion).toList(),
  };

  List<BrowserVerificationStep> _decodeDefinitionSteps(
    Map<String, dynamic> wire,
  ) {
    final assertions = _maps(wire['assertions']);
    return _maps(
      wire['steps'],
    ).where((step) => step['assertionId'] == null).map((step) {
      final decoded = decodeStep(step);
      final attached = assertions
          .where((item) => item['stepId']?.toString() == decoded.id)
          .map(decodeAssertion)
          .toList();
      return decoded.copyWith(
        assertions: attached.isEmpty ? decoded.assertions : attached,
      );
    }).toList();
  }

  BrowserAssertion decodeAssertion(Map<String, dynamic> wire) =>
      BrowserAssertion(
        id: wire['id']?.toString() ?? '',
        kind: _assertionKind(wire['kind']),
        target: wire['target']?.toString(),
        expected: wire['expected']?.toString() ?? '',
        required: wire['required'] != false,
      );

  Map<String, dynamic> encodeAssertion(BrowserAssertion value) => {
    'id': value.id,
    'kind': value.kind.name,
    'target': value.target,
    'expected': value.expected,
    'required': value.required,
  };

  BrowserVerificationRun decodeRun(
    Map<String, dynamic> wire, {
    String? Function(String hash)? artifactPath,
  }) {
    final durableRun = _map(wire['run']);
    final run = durableRun.isEmpty ? wire : durableRun;
    final result = _map(run['result']);
    final definition = _map(wire['definition']);
    Object? section(String key) => wire[key] ?? result[key] ?? run[key];
    final variants = <Map<String, dynamic>>[
      ..._maps(definition['variants']),
      ..._maps(section('variants')),
    ];
    List<Map<String, dynamic>> variantMaps(String key) => [
      ..._maps(section(key)),
      for (final variant in variants) ..._maps(variant[key]),
    ];
    final accessibility = section('accessibility');
    final accessibilityItems = <Map<String, dynamic>>[
      ...(accessibility is Map
          ? _maps(_map(accessibility)['issues'])
          : _maps(accessibility ?? wire['accessibilityIssues'])),
      for (final variant in variants)
        ..._maps(_map(variant['accessibility'])['issues']),
    ];
    final console = variantMaps('console');
    final pageErrors = variantMaps('pageErrors');
    final consoleErrors = <String>{
      ..._strings(wire['consoleErrors']),
      ..._strings(section('console')),
      ...console
          .where(
            (entry) =>
                entry['level']?.toString() == 'error' ||
                entry['level']?.toString() == 'critical',
          )
          .map((entry) => entry['message']?.toString() ?? entry.toString()),
      ..._strings(section('pageErrors')),
      ...pageErrors.map(
        (entry) => entry['message']?.toString() ?? entry.toString(),
      ),
    }.toList();
    final timeline = _maps(
      section('timeline') ?? section('events'),
    ).map(decodeTimeline).toList();
    final assertions = variantMaps('assertions');
    if (!timeline.any((event) => event.kind == BrowserTimelineKind.assertion)) {
      timeline.addAll(assertions.map(_assertionTimeline));
    }
    _appendTimelineSection(
      timeline,
      variantMaps('network'),
      BrowserTimelineKind.network,
      fallbackLabel: 'Network request',
    );
    _appendTimelineSection(
      timeline,
      console,
      BrowserTimelineKind.console,
      fallbackLabel: 'Console message',
    );
    _appendTimelineSection(
      timeline,
      pageErrors,
      BrowserTimelineKind.error,
      fallbackLabel: 'Page error',
    );
    _appendTimelineSection(
      timeline,
      variantMaps(
        'artifacts',
      ).where((item) => item['kind']?.toString() == 'screenshot'),
      BrowserTimelineKind.screenshot,
      fallbackLabel: 'Screenshot captured',
    );
    final visualComparisons = <Map<String, dynamic>>[
      for (final comparison in _maps(section('visualComparisons')))
        {
          ...comparison,
          if (comparison['viewport'] == null)
            'viewport': _variantFor(
              variants,
              comparison['variantKey']?.toString(),
            ),
        },
      for (final variant in variants)
        for (final comparison in _maps(variant['visualComparisons']))
          {
            ...comparison,
            if (comparison['viewport'] == null)
              'viewport': variant['viewport'] ?? variant['variant'] ?? variant,
          },
    ];
    final warningCount = _int(run['warningCount'], 0);
    return BrowserVerificationRun(
      id: run['id']?.toString() ?? result['runId']?.toString() ?? '',
      taskId: run['taskId']?.toString() ?? result['taskId']?.toString() ?? '',
      definitionId:
          run['definitionId']?.toString() ??
          result['definitionId']?.toString() ??
          '',
      status: _runStatus(result['status'] ?? run['status']),
      startedAt:
          _date(run['startedAt'] ?? result['startedAt'] ?? run['createdAt']) ??
          DateTime.now(),
      completedAt: _date(run['completedAt'] ?? result['completedAt']),
      attempt: _int(run['attempt'] ?? result['attempt'], 1),
      timeline: timeline,
      visualComparisons: visualComparisons
          .map((item) => decodeComparison(item, artifactPath: artifactPath))
          .toList(),
      accessibilityIssues: accessibilityItems
          .map(decodeAccessibilityIssue)
          .toList(),
      consoleErrors: consoleErrors,
      warningMessages: [
        ..._strings(section('warningMessages')),
        if (warningCount > 0) '$warningCount browser verification warnings',
        if (run['failure'] case final failure?) failure.toString(),
      ],
    );
  }

  Map<String, dynamic> _variantFor(
    List<Map<String, dynamic>> variants,
    String? key,
  ) => variants.firstWhere(
    (variant) =>
        variant['key']?.toString() == key || variant['name']?.toString() == key,
    orElse: () => <String, dynamic>{'key': key ?? 'viewport'},
  );

  BrowserTimelineEvent _assertionTimeline(Map<String, dynamic> wire) =>
      BrowserTimelineEvent(
        id: wire['id']?.toString() ?? '',
        kind: BrowserTimelineKind.assertion,
        label: wire['label']?.toString() ?? 'Browser assertion',
        createdAt: _date(wire['createdAt']) ?? DateTime.now(),
        passed: wire['passed'] as bool? ?? wire['status'] == 'passed',
        details: _map(wire['details']).cast<String, Object?>(),
      );

  void _appendTimelineSection(
    List<BrowserTimelineEvent> timeline,
    Iterable<Map<String, dynamic>> items,
    BrowserTimelineKind kind, {
    required String fallbackLabel,
  }) {
    final knownIds = timeline.map((event) => event.id).toSet();
    for (final item in items) {
      final id =
          '${kind.name}-'
          '${item['id']?.toString() ?? timeline.length}';
      if (!knownIds.add(id)) continue;
      timeline.add(
        BrowserTimelineEvent(
          id: id,
          kind: kind,
          label:
              item['label']?.toString() ??
              item['message']?.toString() ??
              item['url']?.toString() ??
              fallbackLabel,
          createdAt: _date(item['createdAt']) ?? DateTime.now(),
          passed: item['passed'] as bool?,
          duration: item['durationMs'] == null
              ? null
              : Duration(milliseconds: _int(item['durationMs'], 0)),
          details: item.cast<String, Object?>(),
        ),
      );
    }
  }

  BrowserTimelineEvent decodeTimeline(
    Map<String, dynamic> wire,
  ) => BrowserTimelineEvent(
    id: wire['id']?.toString() ?? '',
    kind: _timelineKind(wire['kind']),
    label:
        wire['label']?.toString() ??
        _map(wire['payload'])['message']?.toString() ??
        _map(wire['payload'])['url']?.toString() ??
        _title(wire['kind']?.toString() ?? 'Browser event'),
    createdAt: _date(wire['createdAt']) ?? DateTime.now(),
    details: _map(wire['details'] ?? wire['payload']).cast<String, Object?>(),
    passed:
        wire['passed'] as bool? ?? (wire['severity'] == 'error' ? false : null),
    duration: wire['durationMs'] == null
        ? null
        : Duration(milliseconds: _int(wire['durationMs'], 0)),
  );

  BrowserVisualComparison decodeComparison(
    Map<String, dynamic> wire, {
    String? Function(String hash)? artifactPath,
  }) {
    BrowserVisualArtifact? artifact(String key) {
      final hash = wire[key]?.toString();
      if (hash == null || hash.isEmpty) return null;
      return BrowserVisualArtifact(
        hash: hash,
        localPath: artifactPath?.call(hash),
      );
    }

    return BrowserVisualComparison(
      id: wire['id']?.toString() ?? '',
      viewport: decodeViewport(_map(wire['viewport'])),
      method: wire['perceptualDifferenceRatio'] == null
          ? _visualMethod(wire['method'])
          : VisualComparisonMethod.perceptual,
      status: _visualStatus(wire['status']),
      threshold: _ratio(
        wire['maxDiffPixelRatio'] ??
            wire['perceptualThreshold'] ??
            wire['thresholdRatio'] ??
            wire['threshold'],
      ),
      difference: _ratio(
        wire['diffPixelRatio'] ??
            wire['perceptualDifference'] ??
            wire['perceptualDifferenceRatio'] ??
            wire['pixelDifferenceRatio'] ??
            wire['difference'],
      ),
      createdAt: _date(wire['createdAt']) ?? DateTime.now(),
      baseline: artifact('baselineArtifactHash'),
      current: _artifact(
        wire['currentArtifactHash'] ?? wire['currentHash'],
        artifactPath,
      ),
      diff: _artifact(
        wire['diffArtifactHash'] ?? wire['differenceHash'],
        artifactPath,
      ),
      masks: _maps(
        wire['masks'] ?? wire['ignoreRegions'],
      ).map(decodeMask).toList(),
      approvedBy: wire['approvedBy']?.toString(),
      approvedAt: _date(wire['approvedAt']),
    );
  }

  BrowserVisualArtifact? _artifact(
    Object? value,
    String? Function(String hash)? artifactPath,
  ) {
    final hash = value?.toString();
    if (hash == null || hash.isEmpty) return null;
    return BrowserVisualArtifact(
      hash: hash,
      localPath: artifactPath?.call(hash),
    );
  }

  VisualMaskRegion decodeMask(Map<String, dynamic> wire) => VisualMaskRegion(
    id: wire['id']?.toString() ?? '',
    label: wire['label']?.toString() ?? 'Dynamic region',
    x: _double(wire['x'], 0),
    y: _double(wire['y'], 0),
    width: _double(wire['width'], 0),
    height: _double(wire['height'], 0),
  );

  Map<String, dynamic> encodeMask(VisualMaskRegion value) => {
    'id': value.id,
    'label': value.label,
    'x': value.x,
    'y': value.y,
    'width': value.width,
    'height': value.height,
  };

  BrowserAccessibilityIssue decodeAccessibilityIssue(
    Map<String, dynamic> wire,
  ) => BrowserAccessibilityIssue(
    id: wire['id']?.toString() ?? '',
    category: _accessibilityCategory(wire['category'] ?? wire['ruleId']),
    severity: _accessibilitySeverity(wire['severity']),
    message: wire['message']?.toString() ?? 'Accessibility issue',
    selector: wire['selector']?.toString(),
    help: wire['help']?.toString() ?? wire['helpUrl']?.toString(),
  );
}

final _artifactHash = RegExp(r'^[0-9a-f]{64}$');

bool _isBrowserVerificationEvent(Map<String, dynamic> wire) {
  final kind = _eventEnvelope(wire)['kind']?.toString() ?? '';
  return kind.startsWith('browser.verification.');
}

Map<String, dynamic> _eventEnvelope(Map<String, dynamic> wire) {
  final nested = wire['event'];
  return nested is Map ? nested.cast<String, dynamic>() : wire;
}

Map<String, dynamic> _map(Object? value) =>
    value is Map ? value.cast<String, dynamic>() : <String, dynamic>{};

Map<String, dynamic> _withoutNullValues(Map<String, dynamic> value) => {
  for (final entry in value.entries)
    if (entry.value != null) entry.key: entry.value,
};

List<Map<String, dynamic>> _maps(Object? value) => (value as List? ?? const [])
    .whereType<Map>()
    .map((item) => item.cast<String, dynamic>())
    .toList();

List<String> _strings(Object? value) =>
    (value as List? ?? const []).map((item) => item.toString()).toList();

int _int(Object? value, int fallback) => (value as num?)?.toInt() ?? fallback;
double _double(Object? value, double fallback) =>
    (value as num?)?.toDouble() ?? fallback;

double _ratio(Object? value, {double fallback = 0}) {
  final parsed = value is num ? value.toDouble() : double.tryParse('$value');
  if (parsed == null || !parsed.isFinite || parsed < 0) return fallback;
  if (parsed <= 1) return parsed;
  if (parsed <= 100) return parsed / 100;
  return fallback;
}

DateTime? _date(Object? value) {
  if (value is num) return DateTime.fromMillisecondsSinceEpoch(value.toInt());
  return value is String ? DateTime.tryParse(value) : null;
}

BrowserStepKind _stepKind(Object? value) {
  if (value == 'wait') return BrowserStepKind.waitFor;
  return BrowserStepKind.values.firstWhere(
    (item) => item.name == value?.toString(),
    orElse: () => BrowserStepKind.navigate,
  );
}

BrowserAssertionKind _assertionKind(Object? value) =>
    BrowserAssertionKind.values.firstWhere(
      (item) => item.name == value?.toString(),
      orElse: () => BrowserAssertionKind.element,
    );
BrowserRunStatus _runStatus(Object? value) => switch (value?.toString()) {
  'passed' || 'approved' => BrowserRunStatus.passed,
  'needs_review' => BrowserRunStatus.needsReview,
  'failed' => BrowserRunStatus.failed,
  'cancelled' => BrowserRunStatus.cancelled,
  'running' => BrowserRunStatus.running,
  _ => BrowserRunStatus.queued,
};
BrowserTimelineKind _timelineKind(Object? value) {
  final kind = value?.toString().toLowerCase() ?? '';
  if (kind.contains('navigat')) return BrowserTimelineKind.navigation;
  if (kind.contains('network') || kind.contains('request')) {
    return BrowserTimelineKind.network;
  }
  if (kind.contains('console')) return BrowserTimelineKind.console;
  if (kind.contains('screenshot') || kind.contains('baseline')) {
    return BrowserTimelineKind.screenshot;
  }
  if (kind.contains('assert')) return BrowserTimelineKind.assertion;
  if (kind.contains('takeover')) return BrowserTimelineKind.takeover;
  if (kind.contains('complete') || kind == 'finished') {
    return BrowserTimelineKind.completion;
  }
  if (kind.contains('error') || kind.contains('fail')) {
    return BrowserTimelineKind.error;
  }
  return BrowserTimelineKind.interaction;
}

VisualComparisonMethod _visualMethod(Object? value) =>
    VisualComparisonMethod.values.firstWhere(
      (item) => item.name == value?.toString(),
      orElse: () => VisualComparisonMethod.pixel,
    );
VisualComparisonStatus _visualStatus(Object? value) =>
    switch (value?.toString()) {
      'passed' || 'same' => VisualComparisonStatus.passed,
      'missing_baseline' ||
      'missingBaseline' => VisualComparisonStatus.missingBaseline,
      'approved' => VisualComparisonStatus.approved,
      _ => VisualComparisonStatus.changed,
    };
AccessibilityCategory _accessibilityCategory(Object? value) {
  final rule = value?.toString().toLowerCase() ?? '';
  if (rule.contains('contrast')) return AccessibilityCategory.contrast;
  if (rule.contains('keyboard') || rule.contains('trap')) {
    return AccessibilityCategory.keyboard;
  }
  if (rule.contains('heading')) return AccessibilityCategory.headings;
  if (rule.contains('landmark') || rule.contains('region')) {
    return AccessibilityCategory.landmarks;
  }
  if (rule.contains('form') || rule.contains('input')) {
    return AccessibilityCategory.forms;
  }
  if (rule.contains('focus')) return AccessibilityCategory.focus;
  return AccessibilityCategory.labels;
}

AccessibilitySeverity _accessibilitySeverity(Object? value) =>
    switch (value?.toString()) {
      'critical' || 'serious' || 'error' => AccessibilitySeverity.critical,
      'minor' || 'info' => AccessibilitySeverity.info,
      _ => AccessibilitySeverity.warning,
    };

String _title(String value) => value
    .replaceAll(RegExp(r'[_\-.]+'), ' ')
    .split(' ')
    .where((part) => part.isNotEmpty)
    .map((part) => '${part[0].toUpperCase()}${part.substring(1)}')
    .join(' ');

final _uuid = RegExp(
  r'^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-5][0-9a-fA-F]{3}-'
  r'[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$',
);
