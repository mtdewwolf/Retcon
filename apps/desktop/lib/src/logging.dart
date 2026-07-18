import 'dart:developer' as developer;

import 'package:flutter/foundation.dart';
import 'package:logging/logging.dart';

import 'diagnostics/desktop_diagnostics.dart';

/// Configure structured logging for the desktop shell.
///
/// Phase 1 scope: hierarchical loggers emitting to the developer log (and the
/// console in debug builds). Later phases forward records to the core service
/// so shell logs land in the shared diagnostics store.
void initLogging() {
  Logger.root.level = kDebugMode ? Level.ALL : Level.INFO;
  Logger.root.onRecord.listen((record) {
    DesktopDiagnostics.instance.captureLog(record);
    final component = _safeComponent(record.loggerName);
    final eventCode = record.level >= Level.SEVERE
        ? 'desktop_log_error'
        : 'desktop_log';
    developer.log(
      eventCode,
      time: record.time,
      level: record.level.value,
      name: component,
    );
    if (kDebugMode) {
      // ignore: avoid_print — deliberate console mirror for debug runs.
      print(
        '${record.time.toIso8601String()} ${record.level.name} '
        '[$component] $eventCode',
      );
    }
  });
}

String _safeComponent(String value) =>
    RegExp(r'^[A-Za-z0-9_.-]{1,64}$').hasMatch(value) ? value : 'desktop';
