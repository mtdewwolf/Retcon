import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:retcon_design_system/retcon_design_system.dart';
import 'package:window_manager/window_manager.dart';

import 'core_client.dart';
import 'window_controller.dart';
import 'workspace.dart';

enum ShellCommand {
  newProject('New project', Icons.create_new_folder),
  openProject('Open project', Icons.folder_open),
  commandPalette('Command palette', Icons.search),
  terminal('Open terminal', Icons.terminal),
  browser('Open browser', Icons.language),
  taskBoard('Task board', Icons.view_kanban),
  settings('Settings', Icons.settings),
  diagnostics('Diagnostics', Icons.monitor_heart),
  fullScreen('Toggle full screen', Icons.fullscreen),
  exit('Exit Retcon', Icons.power_settings_new);

  const ShellCommand(this.label, this.icon);
  final String label;
  final IconData icon;
}

class _OpenPaletteIntent extends Intent {
  const _OpenPaletteIntent();
}

class _ToggleFullScreenIntent extends Intent {
  const _ToggleFullScreenIntent();
}

/// The Phase 7 frameless desktop shell.
class DesktopShell extends StatefulWidget {
  const DesktopShell({
    super.key,
    this.core,
    this.windowController = const NativeWindowController(),
    this.projectTitle = 'No project open',
    this.branch = '—',
    this.provider = 'Provider offline',
    this.onCommand,
  });

  final CoreClient? core;
  final RetconWindowController windowController;
  final String projectTitle;
  final String branch;
  final String provider;
  final ValueChanged<ShellCommand>? onCommand;

  @override
  State<DesktopShell> createState() => _DesktopShellState();
}

class _DesktopShellState extends State<DesktopShell> {
  bool _startMenuOpen = false;
  late final WorkspaceController _workspace = WorkspaceController();

  @override
  void dispose() {
    _workspace.dispose();
    super.dispose();
  }

  Future<void> _run(ShellCommand command) async {
    widget.onCommand?.call(command);
    setState(() => _startMenuOpen = false);
    switch (command) {
      case ShellCommand.terminal:
        await _workspace.float(PanelDefinition.terminal, const Size(900, 600));
      case ShellCommand.browser:
        await _workspace.float(PanelDefinition.browser, const Size(900, 600));
      case ShellCommand.commandPalette:
        await _showCommandPalette();
      case ShellCommand.fullScreen:
        await widget.windowController.toggleFullScreen();
      case ShellCommand.exit:
        await widget.windowController.close();
      default:
        break;
    }
  }

  Future<void> _showCommandPalette() => showDialog<void>(
    context: context,
    builder: (context) => _CommandPalette(onSelected: _run),
  );

  @override
  Widget build(BuildContext context) {
    return Shortcuts(
      shortcuts: const {
        SingleActivator(LogicalKeyboardKey.keyP, control: true, shift: true):
            _OpenPaletteIntent(),
        SingleActivator(LogicalKeyboardKey.f11): _ToggleFullScreenIntent(),
      },
      child: Actions(
        actions: {
          _OpenPaletteIntent: CallbackAction<_OpenPaletteIntent>(
            onInvoke: (_) {
              unawaited(_run(ShellCommand.commandPalette));
              return null;
            },
          ),
          _ToggleFullScreenIntent: CallbackAction<_ToggleFullScreenIntent>(
            onInvoke: (_) {
              unawaited(_run(ShellCommand.fullScreen));
              return null;
            },
          ),
        },
        child: Focus(
          autofocus: true,
          child: Scaffold(
            body: Stack(
              children: [
                Column(
                  children: [
                    _TitleBar(
                      core: widget.core,
                      projectTitle: widget.projectTitle,
                      branch: widget.branch,
                      provider: widget.provider,
                      windowController: widget.windowController,
                    ),
                    _ApplicationMenu(onCommand: _run),
                    Expanded(child: _Workspace(controller: _workspace)),
                    _Taskbar(
                      core: widget.core,
                      startMenuOpen: _startMenuOpen,
                      onStartPressed: () =>
                          setState(() => _startMenuOpen = !_startMenuOpen),
                    ),
                  ],
                ),
                if (_startMenuOpen)
                  Positioned(
                    left: 0,
                    bottom: RetconDimensions.taskbarHeight,
                    child: _StartMenu(onCommand: _run),
                  ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _TitleBar extends StatelessWidget {
  const _TitleBar({
    required this.core,
    required this.projectTitle,
    required this.branch,
    required this.provider,
    required this.windowController,
  });

  final CoreClient? core;
  final String projectTitle;
  final String branch;
  final String provider;
  final RetconWindowController windowController;

  @override
  Widget build(BuildContext context) {
    return Container(
      height: RetconDimensions.titleBarHeight,
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
            child: DragToMoveArea(
              child: Padding(
                padding: const EdgeInsets.symmetric(
                  horizontal: RetconSpacing.sm,
                ),
                child: LayoutBuilder(
                  builder: (context, constraints) => Row(
                    children: [
                      const Icon(
                        Icons.auto_awesome,
                        size: RetconIconSizes.standard,
                      ),
                      const SizedBox(width: RetconSpacing.sm),
                      Expanded(
                        child: Text(
                          'Retcon — $projectTitle',
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: Theme.of(context).textTheme.titleMedium,
                        ),
                      ),
                      if (constraints.maxWidth >= 440) ...[
                        const SizedBox(width: RetconSpacing.sm),
                        RetconBadge(label: branch),
                      ],
                      if (constraints.maxWidth >= 700) ...[
                        const SizedBox(width: RetconSpacing.sm),
                        RetconBadge(
                          label: provider,
                          status: RetconStatus.warning,
                        ),
                      ],
                      if (constraints.maxWidth >= 860) ...[
                        const SizedBox(width: RetconSpacing.sm),
                        _CoreStatus(core: core),
                      ],
                    ],
                  ),
                ),
              ),
            ),
          ),
          RetconIconButton(
            label: 'Minimize',
            icon: Icons.remove,
            onPressed: () => unawaited(windowController.minimize()),
          ),
          RetconIconButton(
            label: 'Maximize or restore',
            icon: Icons.crop_square,
            onPressed: () => unawaited(windowController.toggleMaximized()),
          ),
          RetconIconButton(
            label: 'Close',
            icon: Icons.close,
            onPressed: () => unawaited(windowController.close()),
          ),
        ],
      ),
    );
  }
}

class _CoreStatus extends StatelessWidget {
  const _CoreStatus({required this.core});
  final CoreClient? core;

  @override
  Widget build(BuildContext context) {
    if (core == null) {
      return const RetconBadge(
        label: 'Core unavailable',
        status: RetconStatus.error,
      );
    }
    return AnimatedBuilder(
      animation: core!,
      builder: (context, _) {
        final status = core!.status;
        return RetconBadge(
          label: 'Core ${status.name}',
          status: switch (status) {
            CoreConnectionStatus.connected => RetconStatus.success,
            CoreConnectionStatus.connecting ||
            CoreConnectionStatus.reconnecting => RetconStatus.warning,
            CoreConnectionStatus.disconnected => RetconStatus.error,
          },
        );
      },
    );
  }
}

class _ApplicationMenu extends StatelessWidget {
  const _ApplicationMenu({required this.onCommand});
  final ValueChanged<ShellCommand> onCommand;

  @override
  Widget build(BuildContext context) => MenuBar(
    children: [
      _menu('File', [
        ShellCommand.newProject,
        ShellCommand.openProject,
        ShellCommand.exit,
      ]),
      _menu('Edit', [ShellCommand.commandPalette]),
      _menu('View', [ShellCommand.fullScreen, ShellCommand.taskBoard]),
      _menu('Agents', [ShellCommand.commandPalette]),
      _menu('Git', [ShellCommand.commandPalette]),
      _menu('Browser', [ShellCommand.browser]),
      _menu('Tools', [
        ShellCommand.terminal,
        ShellCommand.settings,
        ShellCommand.diagnostics,
      ]),
      _menu('Help', [ShellCommand.commandPalette]),
    ],
  );

  SubmenuButton _menu(String label, List<ShellCommand> commands) =>
      SubmenuButton(
        menuChildren: [
          for (final command in commands)
            MenuItemButton(
              leadingIcon: Icon(command.icon, size: RetconIconSizes.standard),
              onPressed: () => onCommand(command),
              child: Text(command.label),
            ),
        ],
        child: Text(label),
      );
}

class _Workspace extends StatelessWidget {
  const _Workspace({required this.controller});
  final WorkspaceController controller;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.all(RetconSpacing.sm),
    child: Column(
      children: [
        Align(
          alignment: Alignment.centerRight,
          child: Wrap(
            spacing: RetconSpacing.xs,
            children: [
              TextButton.icon(
                onPressed: () => controller.reopenLast(),
                icon: const Icon(Icons.undo),
                label: const Text('Reopen panel'),
              ),
              TextButton.icon(
                onPressed: () => controller.reset(),
                icon: const Icon(Icons.restart_alt),
                label: const Text('Reset layout'),
              ),
            ],
          ),
        ),
        const SizedBox(height: RetconSpacing.xs),
        Expanded(child: DockingWorkspace(controller: controller)),
      ],
    ),
  );
}

class _Taskbar extends StatelessWidget {
  const _Taskbar({
    required this.core,
    required this.startMenuOpen,
    required this.onStartPressed,
  });
  final CoreClient? core;
  final bool startMenuOpen;
  final VoidCallback onStartPressed;

  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (context, constraints) => Container(
      height: RetconDimensions.taskbarHeight,
      decoration: const BoxDecoration(
        gradient: LinearGradient(
          colors: [RetconColors.titleBarTop, RetconColors.titleBarBottom],
        ),
      ),
      child: Row(
        children: [
          Semantics(
            button: true,
            expanded: startMenuOpen,
            child: TextButton.icon(
              onPressed: onStartPressed,
              icon: const Icon(Icons.window),
              label: const Text('start'),
            ),
          ),
          const VerticalDivider(width: RetconSpacing.sm),
          const RetconBadge(label: 'Workspace'),
          const Spacer(),
          if (constraints.maxWidth >= 900) ...[
            const RetconBadge(label: '0 approvals'),
            const SizedBox(width: RetconSpacing.xs),
            const RetconBadge(label: '0 errors'),
            const SizedBox(width: RetconSpacing.sm),
          ],
          _CoreStatus(core: core),
          const SizedBox(width: RetconSpacing.sm),
        ],
      ),
    ),
  );
}

class _StartMenu extends StatelessWidget {
  const _StartMenu({required this.onCommand});
  final ValueChanged<ShellCommand> onCommand;

  @override
  Widget build(BuildContext context) => Material(
    elevation: 12,
    child: SizedBox(
      width: 280,
      child: RetconPanel(
        label: 'Start menu',
        padding: const EdgeInsets.all(RetconSpacing.xs),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            for (final command in ShellCommand.values.where(
              (command) =>
                  command != ShellCommand.commandPalette &&
                  command != ShellCommand.fullScreen,
            ))
              ListTile(
                dense: true,
                leading: Icon(command.icon),
                title: Text(command.label),
                onTap: () => onCommand(command),
              ),
          ],
        ),
      ),
    ),
  );
}

class _CommandPalette extends StatefulWidget {
  const _CommandPalette({required this.onSelected});
  final ValueChanged<ShellCommand> onSelected;

  @override
  State<_CommandPalette> createState() => _CommandPaletteState();
}

class _CommandPaletteState extends State<_CommandPalette> {
  String query = '';

  @override
  Widget build(BuildContext context) {
    final commands = ShellCommand.values
        .where(
          (command) =>
              command.label.toLowerCase().contains(query.toLowerCase()),
        )
        .toList();
    return Dialog(
      child: SizedBox(
        width: 560,
        height: 420,
        child: RetconPanel(
          label: 'Command palette',
          child: Column(
            children: [
              RetconTextField(
                label: 'Search commands',
                hint: 'Type a command…',
                onChanged: (value) => setState(() => query = value),
              ),
              const SizedBox(height: RetconSpacing.sm),
              Expanded(
                child: ListView(
                  children: [
                    for (final command in commands)
                      ListTile(
                        leading: Icon(command.icon),
                        title: Text(command.label),
                        onTap: () {
                          Navigator.of(context).pop();
                          widget.onSelected(command);
                        },
                      ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
