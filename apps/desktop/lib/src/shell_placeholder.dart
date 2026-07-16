import 'package:flutter/material.dart';
import 'package:retcon_design_system/retcon_design_system.dart';
import 'package:window_manager/window_manager.dart';

import 'core_client.dart';

/// Phase 1 placeholder for the Retcon desktop shell.
///
/// Renders the Luna Dark desktop with a single mock window and taskbar so
/// there is something visibly "Retcon" to launch. The real shell — custom
/// frameless window, docking, taskbar, start menu — is Phases 7–8.
class ShellPlaceholder extends StatelessWidget {
  const ShellPlaceholder({super.key, this.core});

  final CoreClient? core;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: Column(
        children: [
          Expanded(
            child: Center(
              child: _MockWindow(
                title: 'Retcon',
                core: core,
                child: Padding(
                  padding: const EdgeInsets.all(RetconSpacing.xl),
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      const Text(
                        'Retcon',
                        style: TextStyle(
                          fontSize: 28,
                          fontWeight: FontWeight.bold,
                          color: RetconColors.text,
                        ),
                      ),
                      const SizedBox(height: RetconSpacing.sm),
                      const Text(
                        'Supervise and verify AI coding agents',
                        style: TextStyle(color: RetconColors.textMuted),
                      ),
                      const SizedBox(height: RetconSpacing.lg),
                      Text(
                        'Phase 1 — engineering foundation.\n'
                        'The desktop shell arrives in Phase 7.',
                        textAlign: TextAlign.center,
                        style: TextStyle(
                          color: RetconColors.textMuted.withValues(alpha: 0.8),
                          fontSize: 12,
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ),
          const _MockTaskbar(),
        ],
      ),
    );
  }
}

/// A bevelled, title-barred window in the Luna Dark style.
class _MockWindow extends StatelessWidget {
  const _MockWindow({required this.title, required this.child, this.core});

  final String title;
  final Widget child;
  final CoreClient? core;

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: BoxDecoration(
        color: RetconColors.windowBackground,
        border: Border(
          top: BorderSide(color: RetconColors.bevelLight),
          left: BorderSide(color: RetconColors.bevelLight),
          right: BorderSide(color: RetconColors.bevelDark, width: 2),
          bottom: BorderSide(color: RetconColors.bevelDark, width: 2),
        ),
        boxShadow: const [
          BoxShadow(
            color: Color(0xAA000000),
            blurRadius: 16,
            offset: Offset(4, 6),
          ),
        ],
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          DragToMoveArea(
            child: Container(
              height: 28,
              padding: const EdgeInsets.symmetric(horizontal: RetconSpacing.sm),
              decoration: const BoxDecoration(
                gradient: LinearGradient(
                  begin: Alignment.topCenter,
                  end: Alignment.bottomCenter,
                  colors: [
                    RetconColors.titleBarTop,
                    RetconColors.titleBarBottom,
                  ],
                ),
              ),
              child: Row(
                children: [
                  if (core != null)
                    AnimatedBuilder(
                      animation: core!,
                      builder: (context, child) => Semantics(
                        label: 'Core connection ${core!.status.name}',
                        child: Container(
                          width: 8,
                          height: 8,
                          margin: const EdgeInsets.only(
                            right: RetconSpacing.sm,
                          ),
                          decoration: BoxDecoration(
                            shape: BoxShape.circle,
                            color:
                                core!.status == CoreConnectionStatus.connected
                                ? RetconColors.success
                                : RetconColors.warning,
                          ),
                        ),
                      ),
                    ),
                  Expanded(
                    child: Text(
                      title,
                      style: const TextStyle(
                        color: RetconColors.text,
                        fontWeight: FontWeight.bold,
                        fontSize: 13,
                      ),
                    ),
                  ),
                  _WindowButton(
                    label: 'Minimize',
                    icon: Icons.remove,
                    onPressed: windowManager.minimize,
                  ),
                  _WindowButton(
                    label: 'Maximize or restore',
                    icon: Icons.crop_square,
                    onPressed: () async {
                      if (await windowManager.isMaximized()) {
                        await windowManager.unmaximize();
                      } else {
                        await windowManager.maximize();
                      }
                    },
                  ),
                  _WindowButton(
                    label: 'Close',
                    icon: Icons.close,
                    onPressed: windowManager.close,
                  ),
                ],
              ),
            ),
          ),
          child,
        ],
      ),
    );
  }
}

class _WindowButton extends StatelessWidget {
  const _WindowButton({
    required this.label,
    required this.icon,
    required this.onPressed,
  });

  final String label;
  final IconData icon;
  final Future<void> Function() onPressed;

  @override
  Widget build(BuildContext context) => Semantics(
    button: true,
    label: label,
    child: IconButton(
      icon: Icon(icon, size: 14),
      padding: EdgeInsets.zero,
      constraints: const BoxConstraints.tightFor(width: 28, height: 24),
      tooltip: label,
      onPressed: onPressed,
    ),
  );
}

/// The Luna Dark taskbar strip with a Start-green block.
class _MockTaskbar extends StatelessWidget {
  const _MockTaskbar();

  @override
  Widget build(BuildContext context) {
    return Container(
      height: 32,
      decoration: const BoxDecoration(
        gradient: LinearGradient(
          begin: Alignment.topCenter,
          end: Alignment.bottomCenter,
          colors: [RetconColors.titleBarTop, RetconColors.titleBarBottom],
        ),
      ),
      child: Row(
        children: [
          Container(
            padding: const EdgeInsets.symmetric(horizontal: RetconSpacing.lg),
            color: RetconColors.startGreen,
            alignment: Alignment.center,
            child: const Text(
              'start',
              style: TextStyle(
                color: RetconColors.text,
                fontWeight: FontWeight.bold,
                fontStyle: FontStyle.italic,
              ),
            ),
          ),
        ],
      ),
    );
  }
}
