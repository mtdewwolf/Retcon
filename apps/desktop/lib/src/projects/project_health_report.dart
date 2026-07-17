import 'package:flutter/material.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

import 'project_models.dart';

/// Read-only health report for a project folder.
class ProjectHealthReport extends StatelessWidget {
  const ProjectHealthReport({
    super.key,
    required this.health,
    this.analysis,
    this.title,
  });

  final ProjectHealth health;
  final Map<String, dynamic>? analysis;
  final String? title;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return RetconPanel(
      label: title ?? 'Project health',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (analysis != null && analysis!.isNotEmpty) ...[
            Text('Analysis', style: theme.textTheme.titleSmall),
            const SizedBox(height: RetconSpacing.xs),
            _AnalysisSummary(analysis: analysis!),
            const SizedBox(height: RetconSpacing.sm),
          ],
          Text('Health checks', style: theme.textTheme.titleSmall),
          const SizedBox(height: RetconSpacing.xs),
          for (final check in health.checks)
            _HealthCheckTile(check: check),
        ],
      ),
    );
  }
}

class _AnalysisSummary extends StatelessWidget {
  const _AnalysisSummary({required this.analysis});
  final Map<String, dynamic> analysis;

  @override
  Widget build(BuildContext context) {
    final chips = <String>[
      ..._stringList('languages'),
      ..._stringList('frameworks'),
      ..._stringList('packageManagers'),
      ..._stringList('testFrameworks'),
    ];
    if (analysis['docker'] == true) chips.add('Docker');
    if (analysis['ci'] == true) chips.add('CI');
    if (analysis['monorepo'] == true) chips.add('Monorepo');
    if (chips.isEmpty) {
      return Text(
        'No stack markers detected yet.',
        style: Theme.of(context).textTheme.bodySmall,
      );
    }
    return Wrap(
      spacing: RetconSpacing.xs,
      runSpacing: RetconSpacing.xs,
      children: [
        for (final label in chips) RetconBadge(label: label),
      ],
    );
  }

  List<String> _stringList(String key) {
    return (analysis[key] as List? ?? const [])
        .map((value) => value.toString())
        .toList();
  }
}

class _HealthCheckTile extends StatelessWidget {
  const _HealthCheckTile({required this.check});
  final HealthCheck check;

  @override
  Widget build(BuildContext context) {
    final color = switch (check.status) {
      'pass' => Colors.greenAccent,
      'error' => Colors.redAccent,
      _ => Colors.amberAccent,
    };
    final icon = switch (check.status) {
      'pass' => Icons.check_circle,
      'error' => Icons.error,
      _ => Icons.warning,
    };
    return Padding(
      padding: const EdgeInsets.only(bottom: RetconSpacing.sm),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(icon, color: color, size: RetconIconSizes.standard),
          const SizedBox(width: RetconSpacing.sm),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(check.name, style: Theme.of(context).textTheme.titleSmall),
                if (check.detail != null && check.detail!.isNotEmpty)
                  Text(check.detail!),
              ],
            ),
          ),
          RetconBadge(
            label: check.status,
            status: switch (check.status) {
              'pass' => RetconStatus.success,
              'error' => RetconStatus.error,
              _ => RetconStatus.warning,
            },
          ),
        ],
      ),
    );
  }
}
