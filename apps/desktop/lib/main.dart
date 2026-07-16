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
import 'src/logging.dart';
import 'src/shell_placeholder.dart';

final _log = Logger('retcon.desktop');
final services = GetIt.instance;

final ThemeData _lunaDarkTheme = buildLunaDarkTheme();
final Set<PointerDeviceKind> _dragDevices = PointerDeviceKind.values.toSet();

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  initLogging();
  services.registerLazySingleton(CoreClient.new);
  await windowManager.ensureInitialized();
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

class RetconApp extends StatefulWidget {
  const RetconApp({super.key, this.core});
  final CoreClient? core;

  @override
  State<RetconApp> createState() => _RetconAppState();
}

class _RetconAppState extends State<RetconApp> {
  late final CoreClient _client = widget.core ?? CoreClient();
  late final GoRouter _router = GoRouter(
    routes: [
      GoRoute(
        path: '/',
        builder: (context, state) => ShellPlaceholder(core: _client),
      ),
    ],
  );

  @override
  Widget build(BuildContext context) {
    return ChangeNotifierProvider.value(
      value: _client,
      child: MaterialApp.router(
        title: 'Retcon',
        debugShowCheckedModeBanner: false,
        theme: _lunaDarkTheme,
        routerConfig: _router,
        supportedLocales: const [Locale('en')],
        localizationsDelegates: const [
          GlobalMaterialLocalizations.delegate,
          GlobalWidgetsLocalizations.delegate,
          GlobalCupertinoLocalizations.delegate,
        ],
        scrollBehavior: const MaterialScrollBehavior().copyWith(
          dragDevices: _dragDevices,
        ),
      ),
    );
  }
}
