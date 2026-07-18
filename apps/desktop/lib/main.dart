import 'dart:ui';

import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:get_it/get_it.dart';
import 'package:go_router/go_router.dart';
import 'package:logging/logging.dart';
import 'package:provider/provider.dart';
import 'package:retcon_design_system/retcon_design_system.dart';
import 'package:window_manager/window_manager.dart';

import 'src/core_client.dart';
import 'src/desktop_shell.dart';
import 'src/diagnostics/desktop_diagnostics.dart';
import 'src/logging.dart';
import 'src/projects/project_controller.dart';
import 'src/storage_recovery_dialog.dart';

final _log = Logger('retcon.desktop');
final services = GetIt.instance;

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  initLogging();
  installDesktopErrorCapture();
  services.registerLazySingleton(CoreClient.new);
  services.registerLazySingleton<ProjectController>(
    () => ProjectController.fromCore(services<CoreClient>()),
  );
  services.registerLazySingleton<ShellState>(
    () => ShellState(services<CoreClient>()),
  );
  await windowManager.ensureInitialized();
  DesktopDiagnostics.instance.bindCore(services<CoreClient>());
  await windowManager.waitUntilReadyToShow(
    const WindowOptions(
      size: Size(1100, 720),
      minimumSize: Size(760, 480),
      center: true,
      titleBarStyle: TitleBarStyle.hidden,
    ),
    () async {
      await windowManager.show();
      await windowManager.focus();
    },
  );
  services<CoreClient>().connect().catchError(
    (Object error) => _log.warning('core connection deferred', error),
  );
  runApp(RetconApp(core: services<CoreClient>()));
}

class RetconApp extends StatelessWidget {
  const RetconApp({super.key, this.core});
  final CoreClient? core;

  @override
  Widget build(BuildContext context) {
    final client = core ?? CoreClient();
    final projectController = services.isRegistered<ProjectController>()
        ? services<ProjectController>()
        : null;
    final shellState = services.isRegistered<ShellState>()
        ? services<ShellState>()
        : null;
    final router = GoRouter(
      routes: [
        GoRoute(
          path: '/',
          builder: (context, state) =>
              DesktopShell(core: client, projectController: projectController),
        ),
      ],
    );
    return MultiProvider(
      providers: [
        ChangeNotifierProvider.value(value: client),
        if (shellState != null)
          ChangeNotifierProvider<ShellState>.value(value: shellState),
      ],
      child: StorageRecoveryGate(
        core: client,
        child: MaterialApp.router(
          title: 'Retcon',
          debugShowCheckedModeBanner: false,
          theme: buildLunaDarkTheme(),
          routerConfig: router,
          supportedLocales: const [Locale('en')],
          localizationsDelegates: const [
            GlobalMaterialLocalizations.delegate,
            GlobalWidgetsLocalizations.delegate,
            GlobalCupertinoLocalizations.delegate,
          ],
          scrollBehavior: const MaterialScrollBehavior().copyWith(
            dragDevices: PointerDeviceKind.values.toSet(),
          ),
        ),
      ),
    );
  }
}
