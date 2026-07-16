import 'package:flutter/material.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

/// Phase 1 placeholder for the Retcon desktop shell.
///
/// Renders the Luna Dark desktop with a single mock window and taskbar so
/// there is something visibly "Retcon" to launch. The real shell — custom
/// frameless window, docking, taskbar, start menu — is Phases 7–8.
class ShellPlaceholder extends StatelessWidget {
  const ShellPlaceholder({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: Column(
        children: [
          Expanded(
            child: Center(
              child: _MockWindow(
                title: 'Retcon',
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
  const _MockWindow({required this.title, required this.child});

  final String title;
  final Widget child;

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
          Container(
            height: 28,
            padding: const EdgeInsets.symmetric(horizontal: RetconSpacing.sm),
            decoration: const BoxDecoration(
              gradient: LinearGradient(
                begin: Alignment.topCenter,
                end: Alignment.bottomCenter,
                colors: [RetconColors.titleBarTop, RetconColors.titleBarBottom],
              ),
            ),
            child: Row(
              children: [
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
              ],
            ),
          ),
          child,
        ],
      ),
    );
  }
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
