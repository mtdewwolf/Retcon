import 'package:flutter/material.dart';

import 'components/controls.dart';
import 'components/data.dart';
import 'components/layout.dart';
import 'components/navigation.dart';
import 'components/overlays.dart';
import 'components/surfaces.dart';
import 'theme.dart';
import 'tokens.dart';

/// Phase 6 component gallery covering the full Luna Dark library.
class RetconComponentGallery extends StatefulWidget {
  const RetconComponentGallery({super.key});

  @override
  State<RetconComponentGallery> createState() => _RetconComponentGalleryState();
}

class _RetconComponentGalleryState extends State<RetconComponentGallery> {
  bool checked = true;
  String radio = 'one';
  String? dropdownValue = 'rust';
  int? selectedRow;
  String? selectedTreeId;
  bool highContrast = false;
  bool reducedMotion = false;
  bool largeTargets = false;

  static const _treeNodes = [
    RetconTreeNodeData(
      id: 'src',
      label: 'src',
      icon: Icons.folder,
      children: [
        RetconTreeNodeData(
          id: 'main.rs',
          label: 'main.rs',
          icon: Icons.description,
        ),
        RetconTreeNodeData(
          id: 'lib.rs',
          label: 'lib.rs',
          icon: Icons.description,
        ),
      ],
    ),
    RetconTreeNodeData(
      id: 'Cargo.toml',
      label: 'Cargo.toml',
      icon: Icons.settings,
    ),
  ];

  ThemeData get _theme => buildLunaDarkTheme(
    highContrast: highContrast,
    reducedMotion: reducedMotion,
    largeTargets: largeTargets,
  );

  @override
  Widget build(BuildContext context) => Theme(
    data: _theme,
    child: Builder(
      builder: (context) => Scaffold(
        appBar: AppBar(
          title: const Text('Luna Dark component gallery'),
          actions: [
            RetconTooltip(
              message: 'Toggle high contrast',
              child: IconButton(
                icon: const Icon(Icons.contrast),
                onPressed: () => setState(() => highContrast = !highContrast),
              ),
            ),
            RetconTooltip(
              message: 'Toggle reduced motion',
              child: IconButton(
                icon: const Icon(Icons.motion_photos_off),
                onPressed: () => setState(() => reducedMotion = !reducedMotion),
              ),
            ),
            RetconTooltip(
              message: 'Toggle large targets',
              child: IconButton(
                icon: const Icon(Icons.accessibility_new),
                onPressed: () => setState(() => largeTargets = !largeTargets),
              ),
            ),
          ],
        ),
        body: ListView(
          key: const Key('gallery-scroll'),
          padding: const EdgeInsets.all(RetconSpacing.xl),
          children: [
            _sectionTitle(context, 'Core controls'),
            Wrap(
              spacing: RetconSpacing.sm,
              runSpacing: RetconSpacing.sm,
              children: [
                RetconButton(
                  label: 'Run task',
                  icon: Icons.play_arrow,
                  onPressed: () => RetconNotification.show(
                    context,
                    message: 'Task started',
                    level: RetconNotificationLevel.success,
                  ),
                ),
                const RetconButton(label: 'Disabled', onPressed: null),
                RetconIconButton(
                  label: 'Settings',
                  icon: Icons.settings,
                  onPressed: () {},
                ),
                const RetconBadge(label: 'Connected', status: RetconStatus.success),
                const RetconBadge(
                  label: 'Approval needed',
                  status: RetconStatus.warning,
                ),
                RetconMenu(
                  label: 'File',
                  icon: Icons.folder,
                  items: [
                    RetconMenuItem(
                      label: 'Open project',
                      icon: Icons.folder_open,
                      onSelected: () {},
                    ),
                    RetconMenuItem(
                      label: 'Exit',
                      icon: Icons.exit_to_app,
                      onSelected: () {},
                    ),
                  ],
                ),
              ],
            ),
            const SizedBox(height: RetconSpacing.lg),
            RetconPanel(
              label: 'Input controls',
              child: Column(
                children: [
                  const RetconTextField(label: 'Project name', hint: 'Retcon'),
                  RetconCheckbox(
                    label: 'Remember this choice',
                    value: checked,
                    onChanged: (value) => setState(() => checked = value ?? false),
                  ),
                  RetconRadioGroup<String>(
                    groupValue: radio,
                    onChanged: (value) => setState(() => radio = value ?? radio),
                    children: const [
                      RetconRadioButton(label: 'First provider', value: 'one'),
                      RetconRadioButton(label: 'Second provider', value: 'two'),
                    ],
                  ),
                  RetconDropdown<String>(
                    label: 'Language',
                    value: dropdownValue,
                    hint: 'Choose a language',
                    items: const [
                      RetconDropdownItem(value: 'rust', label: 'Rust'),
                      RetconDropdownItem(value: 'dart', label: 'Dart'),
                      RetconDropdownItem(value: 'ts', label: 'TypeScript'),
                    ],
                    onChanged: (value) => setState(() => dropdownValue = value),
                  ),
                  const RetconProgressBar(value: 0.65, label: 'Task progress'),
                ],
              ),
            ),
            const SizedBox(height: RetconSpacing.xl),
            _sectionTitle(context, 'Navigation'),
            SizedBox(
              height: 220,
              child: RetconPanel(
                label: 'Tabs',
                child: RetconTabs(
                  tabs: const [
                    RetconTab(label: 'Files', child: Text('File browser')),
                    RetconTab(label: 'Search', child: Text('Search results')),
                    RetconTab(label: 'Git', child: Text('Git status')),
                  ],
                ),
              ),
            ),
            const SizedBox(height: RetconSpacing.lg),
            RetconPanel(
              label: 'Tree view',
              child: RetconTreeView(
                nodes: _treeNodes,
                selectedId: selectedTreeId,
                onSelected: (node) => setState(() => selectedTreeId = node.id),
              ),
            ),
            const SizedBox(height: RetconSpacing.xl),
            _sectionTitle(context, 'Data display'),
            RetconTable(
              columns: const [
                RetconTableColumn(label: 'Name'),
                RetconTableColumn(label: 'Status'),
                RetconTableColumn(label: 'Age', numeric: true),
              ],
              rows: const [
                RetconTableRow(cells: ['retcon-core', 'Running', '2m']),
                RetconTableRow(cells: ['browser-service', 'Idle', '14m']),
                RetconTableRow(cells: ['desktop', 'Building', '45s']),
              ],
              selectedRowIndex: selectedRow,
              onRowSelected: (index) => setState(() => selectedRow = index),
            ),
            const SizedBox(height: RetconSpacing.xl),
            _sectionTitle(context, 'Layout'),
            SizedBox(
              height: 180,
              child: RetconSplitter(
                semanticsLabel: 'Editor split',
                first: RetconPanel(
                  label: 'Editor',
                  child: Text('Editor pane', style: Theme.of(context).textTheme.bodyMedium),
                ),
                second: RetconPanel(
                  label: 'Terminal',
                  child: Text('Terminal pane', style: Theme.of(context).textTheme.bodyMedium),
                ),
              ),
            ),
            const SizedBox(height: RetconSpacing.xl),
            _sectionTitle(context, 'Overlays'),
            Wrap(
              spacing: RetconSpacing.sm,
              runSpacing: RetconSpacing.sm,
              children: [
                RetconButton(
                  label: 'Open dialog',
                  onPressed: () => showRetconDialog<void>(
                    context: context,
                    title: 'Confirm action',
                    content: const Text('Run this task with elevated permissions?'),
                    actions: [
                      RetconButton(
                        label: 'Cancel',
                        onPressed: () => Navigator.of(context).pop(),
                      ),
                      RetconButton(
                        label: 'Confirm',
                        onPressed: () => Navigator.of(context).pop(),
                      ),
                    ],
                  ),
                ),
                RetconButton(
                  label: 'Show notification',
                  onPressed: () => RetconNotification.show(
                    context,
                    message: 'Provider health check complete',
                    level: RetconNotificationLevel.info,
                  ),
                ),
              ],
            ),
            const SizedBox(height: RetconSpacing.lg),
            RetconContextMenu(
              items: [
                RetconMenuItem(
                  label: 'Copy path',
                  icon: Icons.copy,
                  onSelected: () {},
                ),
                RetconMenuItem(
                  label: 'Reveal in explorer',
                  icon: Icons.folder_open,
                  onSelected: () {},
                ),
              ],
              child: RetconPanel(
                label: 'Context menu target',
                child: Text(
                  'Right-click or Shift+F10 for context menu',
                  style: Theme.of(context).textTheme.bodyMedium,
                ),
              ),
            ),
            const SizedBox(height: RetconSpacing.xl),
            _sectionTitle(context, 'Accessibility'),
            RetconPanel(
              label: 'Active preferences',
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    'Use the app-bar icons to toggle high contrast, reduced motion, '
                    'and large targets. Components below reflect the active settings.',
                    style: Theme.of(context).textTheme.bodyMedium,
                  ),
                  const SizedBox(height: RetconSpacing.md),
                  Wrap(
                    spacing: RetconSpacing.sm,
                    runSpacing: RetconSpacing.sm,
                    children: [
                      RetconBadge(
                        label: highContrast ? 'High contrast on' : 'High contrast off',
                        status: highContrast
                            ? RetconStatus.success
                            : RetconStatus.neutral,
                      ),
                      RetconBadge(
                        label: reducedMotion ? 'Reduced motion on' : 'Reduced motion off',
                        status: reducedMotion
                            ? RetconStatus.success
                            : RetconStatus.neutral,
                      ),
                      RetconBadge(
                        label: largeTargets ? 'Large targets on' : 'Large targets off',
                        status: largeTargets
                            ? RetconStatus.success
                            : RetconStatus.neutral,
                      ),
                    ],
                  ),
                  const SizedBox(height: RetconSpacing.lg),
                  RetconButton(
                    label: 'Warning notification',
                    icon: Icons.warning_amber,
                    onPressed: () => RetconNotification.show(
                      context,
                      message: 'Provider quota nearly exhausted',
                      level: RetconNotificationLevel.warning,
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    ),
  );

  Widget _sectionTitle(BuildContext context, String title) => Padding(
    padding: const EdgeInsets.only(bottom: RetconSpacing.lg),
    child: Text(title, style: Theme.of(context).textTheme.headlineSmall),
  );
}
