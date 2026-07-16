import 'package:flutter/material.dart';
import 'package:logging/logging.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

import 'src/logging.dart';
import 'src/shell_placeholder.dart';

final _log = Logger('retcon.desktop');

void main() {
  initLogging();
  _log.info('retcon desktop shell starting');
  runApp(const RetconApp());
}

/// Application root: Luna Dark theme wrapping the placeholder shell.
class RetconApp extends StatelessWidget {
  const RetconApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Retcon',
      debugShowCheckedModeBanner: false,
      theme: buildLunaDarkTheme(),
      home: const ShellPlaceholder(),
    );
  }
}
