import 'dart:developer' as developer;

import 'package:flutter/foundation.dart';
import 'package:logging/logging.dart';

/// Configure structured logging for the desktop shell.
///
/// Phase 1 scope: hierarchical loggers emitting to the developer log (and the
/// console in debug builds). Later phases forward records to the core service
/// so shell logs land in the shared diagnostics store.
void initLogging() {
  Logger.root.level = kDebugMode ? Level.ALL : Level.INFO;
  Logger.root.onRecord.listen((record) {
    developer.log(
      record.message,
      time: record.time,
      level: record.level.value,
      name: record.loggerName,
      error: record.error,
      stackTrace: record.stackTrace,
    );
    if (kDebugMode) {
      // ignore: avoid_print — deliberate console mirror for debug runs.
      print(
        '${record.time.toIso8601String()} ${record.level.name} '
        '[${record.loggerName}] ${record.message}',
      );
    }
  });
}
