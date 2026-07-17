import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:provider/provider.dart';
import 'package:retcon_design_system/retcon_design_system.dart';
import 'package:window_manager/window_manager.dart';

import 'browser/browser.dart';
import 'core_client.dart';
import 'dev_server/dev_server.dart';
import 'projects/project_picker.dart';
import 'projects/project_controller.dart';
import 'provider_doctor_dialog.dart';
import 'tasks/tasks.dart';
import 'verification/verification.dart';
import 'window_controller.dart';
import 'workspace.dart';

enum ShellCommand {
  newProject('New project', Icons.create_new_folder),
  openProject('Open project', Icons.folder_open),
  commandPalette('Command palette', Icons.search),
  approvals('Approval center', Icons.verified_user),
  checkpoints('Checkpoints', Icons.history),
  terminal('Open terminal', Icons.terminal),
  browser('Open browser', Icons.language),
  serverCenter('Dev server center', Icons.dns),
  taskBoard('Task board', Icons.view_kanban),
  settings('Settings', Icons.settings),
  diagnostics('Diagnostics', Icons.monitor_heart),
  fullScreen('Toggle full screen', Icons.fullscreen),
  exit('Exit Retcon', Icons.power_settings_new);

  const ShellCommand(this.label, this.icon);
  final String label;
  final IconData icon;
}

/// Live provider and counter metadata sourced from Retcon Core.
class ShellState extends ChangeNotifier {
  ShellState(this.core) {
    _coreListener = () {
      if (core.status == CoreConnectionStatus.connected) {
        unawaited(refresh());
      } else {
        provider = 'Core offline';
        notifyListeners();
      }
    };
    core.addListener(_coreListener);
    _events = core.events.listen(_onEvent);
    _poll = Timer.periodic(const Duration(seconds: 30), (_) => refresh());
    unawaited(refresh());
  }

  final CoreClient core;
  late final VoidCallback _coreListener;
  StreamSubscription<Map<String, dynamic>>? _events;
  Timer? _poll;

  String provider = 'Provider offline';
  int approvalCount = 0;
  int errorCount = 0;

  Future<void> refresh() async {
    if (core.status != CoreConnectionStatus.connected) {
      provider = 'Core offline';
      notifyListeners();
      return;
    }
    try {
      final doctor = await core.request('provider.doctor');
      provider = _providerLabel(doctor);
      errorCount = _failureCount(
        doctor['checks'] as List<dynamic>? ?? const [],
      );

      final approvals = await core.request(
        'approval.list',
        params: const {'status': 'pending', 'limit': 1},
      );
      approvalCount =
          (approvals['pendingCount'] as num?)?.toInt() ??
          (approvals['approvals'] as List<dynamic>? ?? const []).length;
    } on Object {
      // Keep the last known values when core is briefly unavailable.
    }
    notifyListeners();
  }

  void _onEvent(Map<String, dynamic> event) {
    final envelope = event['event'] as Map<String, dynamic>? ?? event;
    final name =
        envelope['kind']?.toString() ??
        event['name']?.toString() ??
        event['type']?.toString();
    if (name == 'approval.requested' ||
        name == 'approval.decided' ||
        name == 'permission.rule_created' ||
        name == 'permission.rule_deleted') {
      unawaited(refresh());
    }
  }

  static String _providerLabel(Map<String, dynamic> doctor) {
    final name = doctor['provider_name']?.toString() ?? 'Provider';
    final status = doctor['overall_status']?.toString() ?? 'failure';
    final version = doctor['version']?.toString();
    final suffix = switch (status) {
      'ready' => 'ready',
      'warning' => 'needs attention',
      _ => 'offline',
    };
    if (version != null && version.isNotEmpty) {
      return '$name $version ($suffix)';
    }
    return '$name ($suffix)';
  }

  static int _failureCount(List<dynamic> checks) => checks.where((check) {
    if (check is! Map) return false;
    return check['status']?.toString() == 'failure';
  }).length;

  @override
  void dispose() {
    _poll?.cancel();
    _events?.cancel();
    core.removeListener(_coreListener);
    super.dispose();
  }
}

ShellState? shellStateOf(BuildContext context) {
  try {
    return context.watch<ShellState>();
  } on ProviderNotFoundException {
    return null;
  }
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
    this.projectController,
    this.windowController = const NativeWindowController(),
    this.projectTitle = 'No project open',
    this.branch = '—',
    this.provider = 'Provider offline',
    this.taskRepository,
    this.verificationRepository,
    this.devServerRepository,
    this.browserRepository,
    this.onCommand,
  });

  final CoreClient? core;
  final ProjectController? projectController;
  final RetconWindowController windowController;
  final String projectTitle;
  final String branch;
  final String provider;
  final TaskRepository? taskRepository;
  final VerificationRepository? verificationRepository;
  final DevServerRepository? devServerRepository;
  final BrowserRepository? browserRepository;
  final ValueChanged<ShellCommand>? onCommand;

  @override
  State<DesktopShell> createState() => _DesktopShellState();
}

class _DesktopShellState extends State<DesktopShell> {
  bool _startMenuOpen = false;
  late final WorkspaceController _workspace = WorkspaceController();
  late final InMemoryTaskRepository _offlineTasks =
      InMemoryTaskRepository.demo();
  late final InMemoryVerificationRepository _offlineVerification =
      InMemoryVerificationRepository.demo();
  late final InMemoryDevServerRepository _offlineDevServers =
      InMemoryDevServerRepository.demo();
  late final InMemoryBrowserRepository _offlineBrowser =
      InMemoryBrowserRepository.demo();
  CoreTaskRepository? _coreTasks;
  CoreVerificationRepository? _coreVerification;
  CoreDevServerRepository? _coreDevServers;
  CoreBrowserRepository? _coreBrowser;
  DevServerController? _devServerController;
  DevServerRepository? _devServerControllerRepository;

  @override
  void initState() {
    super.initState();
    final core = widget.core;
    if (core != null) {
      _workspace.bindRpcStore(
        core,
        projectId: widget.projectController?.current?.id,
      );
    }
    widget.projectController?.addListener(_handleProjectUpdate);
  }

  @override
  void didUpdateWidget(covariant DesktopShell oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.projectController != widget.projectController) {
      oldWidget.projectController?.removeListener(_handleProjectUpdate);
      widget.projectController?.addListener(_handleProjectUpdate);
    }
    if (oldWidget.core != widget.core) {
      _coreTasks = null;
      _coreVerification = null;
      _coreDevServers = null;
      unawaited(_coreBrowser?.dispose());
      _coreBrowser = null;
      _devServerController?.dispose();
      _devServerController = null;
      _devServerControllerRepository = null;
    }
    if (oldWidget.devServerRepository != widget.devServerRepository) {
      _devServerController?.dispose();
      _devServerController = null;
      _devServerControllerRepository = null;
    }
    if (oldWidget.browserRepository != widget.browserRepository) {
      unawaited(_coreBrowser?.dispose());
      _coreBrowser = null;
    }
  }

  @override
  void dispose() {
    widget.projectController?.removeListener(_handleProjectUpdate);
    _devServerController?.dispose();
    unawaited(_coreBrowser?.dispose());
    _workspace.dispose();
    super.dispose();
  }

  void _handleProjectUpdate() {
    final core = widget.core;
    if (core != null) {
      _workspace.bindRpcStore(
        core,
        projectId: widget.projectController?.current?.id,
      );
    }
    _devServerController?.dispose();
    _devServerController = null;
    _devServerControllerRepository = null;
    if (mounted) setState(() {});
  }

  String get _projectTitle =>
      widget.projectController?.projectTitle ?? widget.projectTitle;

  String get _branch => widget.projectController?.branch ?? widget.branch;

  Future<void> _run(ShellCommand command) async {
    widget.onCommand?.call(command);
    setState(() => _startMenuOpen = false);
    switch (command) {
      case ShellCommand.approvals:
        await _workspace.openPanel(PanelDefinition.approvals);
      case ShellCommand.checkpoints:
        await _workspace.openPanel(PanelDefinition.checkpoints);
      case ShellCommand.terminal:
        await _workspace.float(PanelDefinition.terminal, const Size(900, 600));
      case ShellCommand.browser:
        await _workspace.openPanel(PanelDefinition.browser);
      case ShellCommand.serverCenter:
        await DevServerDialog.show(
          context,
          controller: _serverController,
          title: _projectTitle,
        );
      case ShellCommand.commandPalette:
        await _showCommandPalette();
      case ShellCommand.settings:
        await _showProviderDoctor();
      case ShellCommand.diagnostics:
        await _showDiagnosticsDialog();
      case ShellCommand.taskBoard:
        await TaskBoardDialog.show(
          context,
          repository: _taskRepository,
          verificationRepository: _verificationRepository,
          devServerRepository: _devServerRepository,
          onOpenPreview: _openBrowserPreview,
          projectId: widget.projectController?.current?.id,
          projectPath:
              widget.projectController?.current?.metadata.repositoryPath,
        );
      case ShellCommand.fullScreen:
        await widget.windowController.toggleFullScreen();
      case ShellCommand.exit:
        await widget.windowController.close();
      case ShellCommand.openProject:
        if (widget.projectController != null) {
          await showProjectPickerFlow(
            context,
            controller: widget.projectController!,
          );
        } else {
          await _showOpenProjectFallback();
        }
      case ShellCommand.newProject:
        if (widget.projectController != null) {
          await showProjectPickerFlow(
            context,
            controller: widget.projectController!,
            cloneFirst: true,
          );
        } else {
          await _showNewProjectStub();
        }
    }
  }

  TaskRepository get _taskRepository {
    final override = widget.taskRepository;
    if (override != null) return override;
    final core = widget.core;
    if (core == null || core.status != CoreConnectionStatus.connected) {
      return _offlineTasks;
    }
    return _coreTasks ??= CoreTaskRepository.fromCore(core);
  }

  VerificationRepository get _verificationRepository {
    final override = widget.verificationRepository;
    if (override != null) return override;
    final core = widget.core;
    if (core == null || core.status != CoreConnectionStatus.connected) {
      return _offlineVerification;
    }
    return _coreVerification ??= CoreVerificationRepository.fromCore(core);
  }

  DevServerRepository get _devServerRepository {
    final override = widget.devServerRepository;
    if (override != null) return override;
    final core = widget.core;
    if (core == null || core.status != CoreConnectionStatus.connected) {
      return _offlineDevServers;
    }
    return _coreDevServers ??= CoreDevServerRepository.fromCore(core);
  }

  BrowserRepository get _browserRepository {
    final override = widget.browserRepository;
    if (override != null) return override;
    final core = widget.core;
    if (core == null || core.status != CoreConnectionStatus.connected) {
      return _offlineBrowser;
    }
    return _coreBrowser ??= CoreBrowserRepository.fromCore(core);
  }

  DevServerController get _serverController {
    final repository = _devServerRepository;
    final existing = _devServerController;
    if (existing != null &&
        identical(_devServerControllerRepository, repository)) {
      return existing;
    }
    existing?.dispose();
    final project = widget.projectController?.current;
    final controller = DevServerController(
      repository: repository,
      projectId: project?.id ?? 'local-project',
      worktreePath: project?.metadata.repositoryPath ?? '',
      onOpenPreview: _openBrowserPreview,
    );
    _devServerController = controller;
    _devServerControllerRepository = repository;
    unawaited(controller.load());
    return controller;
  }

  Future<void> _openBrowserPreview(DevServerPreviewMetadata preview) async {
    await _workspace.openPanel(PanelDefinition.browser);
    final repository = _browserRepository;
    final current = await repository.load();
    if (current.session == null ||
        current.session?.status == BrowserRuntimeStatus.crashed) {
      await repository.launch();
    }
    await repository.navigate(
      preview.url,
      metadata: {
        ...preview.metadata,
        'port': preview.port,
        'serverStatus': preview.status.name,
        'source': 'dev-server-preview',
      },
    );
  }

  Future<void> _showNewProjectStub() => showDialog<void>(
    context: context,
    builder: (context) => AlertDialog(
      title: const Text('New project'),
      content: const Text(
        'Blank project creation (project.init) is not available yet. '
        'Use Clone repository or Open project to register an existing folder.',
      ),
      actions: [
        FilledButton(
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Close'),
        ),
      ],
    ),
  );

  Future<void> _showOpenProjectFallback() async {
    final controller = TextEditingController();
    final path = await showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Open project'),
        content: RetconTextField(
          label: 'Project folder',
          hint: r'C:\projects\my-app',
          controller: controller,
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(controller.text.trim()),
            child: const Text('Open'),
          ),
        ],
      ),
    );
    if (path == null || path.isEmpty || widget.core == null) return;
    try {
      await widget.core!.request('project.open', params: {'path': path});
    } catch (error) {
      if (!mounted) return;
      await showDialog<void>(
        context: context,
        builder: (context) => AlertDialog(
          title: const Text('Could not open project'),
          content: Text(error.toString()),
          actions: [
            FilledButton(
              onPressed: () => Navigator.of(context).pop(),
              child: const Text('Close'),
            ),
          ],
        ),
      );
    }
  }

  Future<void> _showDiagnosticsDialog() async {
    Map<String, dynamic>? report;
    Object? error;
    final core = widget.core;
    if (core != null && core.status == CoreConnectionStatus.connected) {
      try {
        report = await core.storageStatus();
      } catch (caught) {
        error = caught;
      }
    }
    if (!mounted) return;
    await showDialog<void>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Diagnostics'),
        content: SizedBox(
          width: 620,
          child: error != null
              ? Text('Could not load diagnostics: $error')
              : report == null
              ? const Text('Retcon Core is unavailable.')
              : SingleChildScrollView(
                  child: SelectableText(
                    const JsonEncoder.withIndent('  ').convert(report),
                  ),
                ),
        ),
        actions: [
          if (report != null)
            TextButton(
              onPressed: () => Clipboard.setData(
                ClipboardData(
                  text: const JsonEncoder.withIndent('  ').convert(report),
                ),
              ),
              child: const Text('Copy'),
            ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(),
            child: const Text('Close'),
          ),
        ],
      ),
    );
  }

  Future<void> _showCommandPalette() => showDialog<void>(
    context: context,
    builder: (context) => _CommandPalette(onSelected: _run),
  );

  Future<void> _showProviderDoctor() => showDialog<void>(
    context: context,
    builder: (context) => ProviderDoctorDialog(core: widget.core),
  );

  @override
  Widget build(BuildContext context) {
    final shell = shellStateOf(context);
    final projectTitle = _projectTitle;
    final branch = _branch;
    final provider = shell?.provider ?? widget.provider;
    final approvalCount = shell?.approvalCount ?? 0;
    final errorCount = shell?.errorCount ?? 0;

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
                      projectTitle: projectTitle,
                      branch: branch,
                      provider: provider,
                      windowController: widget.windowController,
                    ),
                    _ApplicationMenu(onCommand: _run),
                    Expanded(
                      child: _Workspace(
                        controller: _workspace,
                        core: widget.core,
                        browserRepository: _browserRepository,
                        workingDirectory: widget
                            .projectController
                            ?.current
                            ?.metadata
                            .repositoryPath,
                        projectId: widget.projectController?.current?.id,
                      ),
                    ),
                    _Taskbar(
                      core: widget.core,
                      approvalCount: approvalCount,
                      errorCount: errorCount,
                      startMenuOpen: _startMenuOpen,
                      onStartPressed: () =>
                          setState(() => _startMenuOpen = !_startMenuOpen),
                      onApprovalsPressed: () =>
                          unawaited(_run(ShellCommand.approvals)),
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
      _menu('View', [
        ShellCommand.fullScreen,
        ShellCommand.taskBoard,
        ShellCommand.checkpoints,
      ]),
      _menu('Agents', [ShellCommand.approvals, ShellCommand.commandPalette]),
      _menu('Git', [ShellCommand.commandPalette]),
      _menu('Browser', [ShellCommand.browser, ShellCommand.serverCenter]),
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
  const _Workspace({
    required this.controller,
    this.core,
    this.browserRepository,
    this.workingDirectory,
    this.projectId,
  });
  final WorkspaceController controller;
  final CoreClient? core;
  final BrowserRepository? browserRepository;
  final String? workingDirectory;
  final String? projectId;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.all(RetconSpacing.sm),
    child: Column(
      children: [
        Align(
          alignment: Alignment.centerRight,
          child: Wrap(
            spacing: RetconSpacing.xs,
            runSpacing: RetconSpacing.xs,
            children: [
              for (final preset in LayoutPreset.values)
                TextButton(
                  onPressed: () => controller.applyPreset(preset),
                  child: Text(preset.label),
                ),
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
        Expanded(
          child: DockingWorkspace(
            controller: controller,
            core: core,
            browserRepository: browserRepository,
            workingDirectory: workingDirectory,
            projectId: projectId,
          ),
        ),
      ],
    ),
  );
}

class _Taskbar extends StatelessWidget {
  const _Taskbar({
    required this.core,
    required this.approvalCount,
    required this.errorCount,
    required this.startMenuOpen,
    required this.onStartPressed,
    required this.onApprovalsPressed,
  });
  final CoreClient? core;
  final int approvalCount;
  final int errorCount;
  final bool startMenuOpen;
  final VoidCallback onStartPressed;
  final VoidCallback onApprovalsPressed;

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
            InkWell(
              onTap: onApprovalsPressed,
              child: RetconBadge(
                label: '$approvalCount approvals',
                status: approvalCount == 0
                    ? RetconStatus.neutral
                    : RetconStatus.warning,
              ),
            ),
            const SizedBox(width: RetconSpacing.xs),
            RetconBadge(
              label: '$errorCount errors',
              status: errorCount == 0
                  ? RetconStatus.success
                  : RetconStatus.error,
            ),
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
