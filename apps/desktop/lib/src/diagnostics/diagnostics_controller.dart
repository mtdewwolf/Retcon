import 'package:flutter/foundation.dart';

import 'desktop_diagnostics.dart';
import 'diagnostics_models.dart';
import 'diagnostics_repository.dart';

class DiagnosticsController extends ChangeNotifier {
  DiagnosticsController({
    required DiagnosticsRepository repository,
    DesktopDiagnostics? desktopDiagnostics,
  }) : _repository = repository,
       _desktopDiagnostics = desktopDiagnostics ?? DesktopDiagnostics.instance;

  final DiagnosticsRepository _repository;
  final DesktopDiagnostics _desktopDiagnostics;

  DiagnosticsSnapshot? snapshot;
  SupportBundleReceipt? lastBundle;
  bool loading = false;
  bool updatingPrivacy = false;
  bool exporting = false;
  bool deleting = false;
  String? error;

  Future<void> load() async {
    loading = true;
    error = null;
    notifyListeners();
    try {
      snapshot = await _repository.load();
      _desktopDiagnostics.setTelemetryEnabled(
        snapshot!.privacy.telemetryEnabled,
      );
    } on Object {
      _recordFailure('diagnostics_load_failed');
      error = 'Diagnostics are temporarily unavailable. Try again.';
    } finally {
      loading = false;
      notifyListeners();
    }
  }

  Future<void> setTelemetry(bool enabled) async {
    if (updatingPrivacy) return;
    updatingPrivacy = true;
    error = null;
    notifyListeners();
    try {
      final privacy = await _repository.setTelemetry(enabled);
      final current = snapshot;
      if (current != null) {
        snapshot = DiagnosticsSnapshot(
          overview: current.overview,
          ipc: current.ipc,
          uiFrames: current.uiFrames,
          errors: current.errors,
          processes: current.processes,
          sessions: current.sessions,
          ports: current.ports,
          privacy: privacy,
        );
      }
      _desktopDiagnostics.setTelemetryEnabled(privacy.telemetryEnabled);
    } on Object {
      _recordFailure('diagnostics_privacy_update_failed');
      error = 'Could not update telemetry. Your previous setting is unchanged.';
    } finally {
      updatingPrivacy = false;
      notifyListeners();
    }
  }

  Future<void> exportSupportBundle({String context = 'diagnostics'}) async {
    if (exporting) return;
    exporting = true;
    error = null;
    notifyListeners();
    try {
      await _desktopDiagnostics.flush();
      lastBundle = await _repository.exportSupportBundle(context: context);
    } on Object {
      _recordFailure('diagnostics_bundle_failed');
      error = 'Could not create the support bundle. Try again.';
    } finally {
      exporting = false;
      notifyListeners();
    }
  }

  Future<Map<String, int>?> deleteDiagnosticData() async {
    if (deleting) return null;
    deleting = true;
    error = null;
    notifyListeners();
    try {
      final deleted = await _repository.deleteDiagnosticData();
      _desktopDiagnostics.clearBufferedData();
      lastBundle = null;
      await load();
      return deleted;
    } on Object {
      _recordFailure('diagnostics_delete_failed');
      error = 'Could not delete diagnostic data. Try again.';
      return null;
    } finally {
      deleting = false;
      notifyListeners();
    }
  }

  void _recordFailure(String code) {
    _desktopDiagnostics.captureOperationFailure(
      component: 'desktop.diagnostics',
      code: code,
    );
  }
}
