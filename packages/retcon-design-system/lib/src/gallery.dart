import 'package:flutter/material.dart';

import 'components/controls.dart';
import 'components/surfaces.dart';
import 'tokens.dart';

/// Initial Phase 6 gallery. New components must be represented here when added.
class RetconComponentGallery extends StatefulWidget {
  const RetconComponentGallery({super.key});

  @override
  State<RetconComponentGallery> createState() => _RetconComponentGalleryState();
}

class _RetconComponentGalleryState extends State<RetconComponentGallery> {
  bool checked = true;
  String radio = 'one';

  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(title: const Text('Luna Dark component gallery')),
    body: ListView(
      padding: const EdgeInsets.all(RetconSpacing.xl),
      children: [
        Text('Core controls', style: Theme.of(context).textTheme.headlineSmall),
        const SizedBox(height: RetconSpacing.lg),
        Wrap(
          spacing: RetconSpacing.sm,
          runSpacing: RetconSpacing.sm,
          children: [
            RetconButton(
              label: 'Run task',
              icon: Icons.play_arrow,
              onPressed: () {},
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
              const RetconProgressBar(value: 0.65, label: 'Task progress'),
            ],
          ),
        ),
      ],
    ),
  );
}
