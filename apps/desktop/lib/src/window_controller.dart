import 'package:window_manager/window_manager.dart';

/// Testable boundary around native desktop window operations.
abstract interface class RetconWindowController {
  Future<void> minimize();
  Future<void> toggleMaximized();
  Future<void> close();
  Future<void> toggleFullScreen();
}

/// Production window operations implemented by `window_manager`.
final class NativeWindowController implements RetconWindowController {
  const NativeWindowController();

  @override
  Future<void> minimize() => windowManager.minimize();

  @override
  Future<void> toggleMaximized() async {
    if (await windowManager.isMaximized()) {
      await windowManager.unmaximize();
    } else {
      await windowManager.maximize();
    }
  }

  @override
  Future<void> close() => windowManager.close();

  @override
  Future<void> toggleFullScreen() async {
    await windowManager.setFullScreen(!await windowManager.isFullScreen());
  }
}
