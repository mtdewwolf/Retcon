import 'package:flutter/material.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

import 'controllers/test_dashboard_controller.dart';
import 'ui/test_dashboard_page.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  final controller = TestDashboardController();
  await controller.initialize();
  runApp(TestDashboardApp(controller: controller));
}

class TestDashboardApp extends StatelessWidget {
  const TestDashboardApp({required this.controller, super.key});

  final TestDashboardController controller;

  @override
  Widget build(BuildContext context) => MaterialApp(
    title: 'Retcon Test Dashboard',
    debugShowCheckedModeBanner: false,
    theme: buildLunaDarkTheme(),
    home: Scaffold(
      body: SafeArea(
        child: Padding(
          padding: const EdgeInsets.all(RetconSpacing.sm),
          child: TestDashboardPage(controller: controller),
        ),
      ),
    ),
  );
}
