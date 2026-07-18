import 'diagnostics_models.dart';

abstract interface class DiagnosticsRepository {
  Future<DiagnosticsSnapshot> load({int recentErrorLimit = 50});

  Future<DiagnosticPrivacy> loadPrivacy();

  Future<void> ingestLogs(List<DiagnosticIngestRecord> records);

  Future<void> ingestMetrics(List<DiagnosticMetricRecord> metrics);

  Future<DiagnosticPrivacy> setTelemetry(bool enabled);

  Future<SupportBundleReceipt> exportSupportBundle({
    String context = 'diagnostics',
  });

  Future<Map<String, int>> deleteDiagnosticData();
}
