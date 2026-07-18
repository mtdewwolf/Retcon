import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_desktop/src/projects/project_controller.dart';
import 'package:retcon_desktop/src/projects/project_health_report.dart';
import 'package:retcon_desktop/src/projects/project_models.dart';
import 'package:retcon_desktop/src/projects/project_picker.dart';
import 'package:retcon_desktop/src/projects/project_service.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

void main() {
  group('Project models', () {
    test('parses open project payload', () {
      final opened = OpenProjectResult.fromJson({
        'id': 'abc',
        'metadata': {
          'name': 'Retcon',
          'repositoryPath': 'C:\\Projects\\Retcon',
          'pinned': true,
        },
        'analysis': {
          'languages': ['Rust'],
          'frameworks': [],
          'packageManagers': ['Cargo'],
          'testFrameworks': [],
          'docker': false,
          'ci': true,
          'monorepo': true,
        },
        'health': {
          'checks': [
            {'name': 'Branch detected', 'status': 'pass', 'detail': 'main'},
            {'name': 'Working tree', 'status': 'pass', 'detail': 'clean'},
          ],
        },
      });

      expect(opened.metadata.name, 'Retcon');
      expect(opened.branch, 'main');
      expect(opened.health.checks, hasLength(2));
    });
  });

  group('ProjectController', () {
    late FakeProjectService service;
    late ProjectController controller;

    setUp(() {
      service = FakeProjectService();
      controller = ProjectController(service: service);
    });

    tearDown(() => controller.dispose());

    test('refresh sorts pinned projects first', () async {
      service.projects = [
        ProjectSummary(
          id: '1',
          metadata: const ProjectMetadata(
            name: 'Recent',
            repositoryPath: 'C:\\recent',
          ),
          updatedAt: '2026-07-16T10:00:00Z',
        ),
        ProjectSummary(
          id: '2',
          metadata: const ProjectMetadata(
            name: 'Pinned',
            repositoryPath: 'C:\\pinned',
            pinned: true,
          ),
          updatedAt: '2026-07-15T10:00:00Z',
        ),
      ];

      await controller.refresh();

      expect(controller.projects.first.name, 'Pinned');
      expect(controller.pinnedProjects, hasLength(1));
      expect(controller.recentProjects, hasLength(1));
    });

    test('open updates shell-facing title and branch', () async {
      service.openResult = OpenProjectResult(
        id: '1',
        metadata: const ProjectMetadata(
          name: 'Workspace',
          repositoryPath: 'C:\\workspace',
        ),
        analysis: const {},
        health: const ProjectHealth(
          checks: [
            HealthCheck(
              name: 'Branch detected',
              status: 'pass',
              detail: 'feature/projects',
            ),
          ],
        ),
      );

      await controller.open('C:\\workspace');

      expect(controller.projectTitle, 'Workspace');
      expect(controller.branch, 'feature/projects');
      expect(controller.current?.id, '1');
    });

    test('toggle pin delegates to updateMetadata', () async {
      service.projects = [
        ProjectSummary(
          id: '1',
          metadata: const ProjectMetadata(
            name: 'App',
            repositoryPath: 'C:\\app',
          ),
        ),
      ];

      await controller.refresh();
      await controller.togglePinned(controller.projects.first);

      expect(service.lastMetadataPatch, {'pinned': true});
    });
  });

  testWidgets('project picker lists pinned and recent sections', (
    tester,
  ) async {
    final service = FakeProjectService()
      ..projects = [
        ProjectSummary(
          id: '1',
          metadata: const ProjectMetadata(
            name: 'Pinned app',
            repositoryPath: 'C:\\pinned',
            pinned: true,
          ),
        ),
        ProjectSummary(
          id: '2',
          metadata: const ProjectMetadata(
            name: 'Recent app',
            repositoryPath: 'C:\\recent',
          ),
        ),
      ];
    final controller = ProjectController(service: service);

    await tester.pumpWidget(
      MaterialApp(
        theme: buildLunaDarkTheme(),
        home: Builder(
          builder: (context) => Scaffold(
            body: Center(
              child: FilledButton(
                onPressed: () =>
                    ProjectPickerDialog.show(context, controller: controller),
                child: const Text('Open picker'),
              ),
            ),
          ),
        ),
      ),
    );
    await tester.tap(find.text('Open picker'));
    await tester.pumpAndSettle();

    expect(find.text('Pinned'), findsOneWidget);
    expect(find.text('Recent'), findsOneWidget);
    expect(find.text('Pinned app'), findsOneWidget);
    expect(find.text('Recent app'), findsOneWidget);

    controller.dispose();
  });

  testWidgets('health report renders checks and analysis chips', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: buildLunaDarkTheme(),
        home: Scaffold(
          body: ProjectHealthReport(
            title: 'Health',
            analysis: const {
              'languages': ['Rust'],
              'frameworks': ['Flutter'],
              'packageManagers': ['Cargo'],
              'testFrameworks': [],
              'docker': true,
              'ci': false,
              'monorepo': false,
            },
            health: const ProjectHealth(
              checks: [
                HealthCheck(name: 'Git installed', status: 'pass'),
                HealthCheck(
                  name: 'Working tree',
                  status: 'warning',
                  detail: 'uncommitted changes',
                ),
              ],
            ),
          ),
        ),
      ),
    );

    expect(find.text('Health checks'), findsOneWidget);
    expect(find.text('Rust'), findsOneWidget);
    expect(find.text('Flutter'), findsOneWidget);
    expect(find.text('Docker'), findsOneWidget);
    expect(find.text('Git installed'), findsOneWidget);
    expect(find.text('uncommitted changes'), findsOneWidget);
  });
}

class FakeProjectService implements ProjectService {
  List<ProjectSummary> projects = const [];
  OpenProjectResult? openResult;
  Map<String, dynamic>? lastMetadataPatch;

  @override
  Future<OpenProjectResult> clone({
    required String remoteUrl,
    required String destination,
  }) async =>
      openResult ??
      OpenProjectResult(
        id: 'clone',
        metadata: ProjectMetadata(
          name: destination.split(r'\').last,
          repositoryPath: destination,
          remoteUrl: remoteUrl,
        ),
        analysis: const {},
        health: const ProjectHealth(checks: []),
      );

  @override
  Future<ProjectHealth> inspect(String path) async =>
      const ProjectHealth(checks: []);

  @override
  Future<List<ProjectSummary>> list({String? query}) async => projects;

  @override
  Future<OpenProjectResult> open(String path) async =>
      openResult ??
      OpenProjectResult(
        id: 'open',
        metadata: ProjectMetadata(
          name: path.split(r'\').last,
          repositoryPath: path,
        ),
        analysis: const {},
        health: const ProjectHealth(checks: []),
      );

  @override
  Future<void> remove(String projectId) async {}

  @override
  Future<ProjectMetadata> updateMetadata(
    String projectId,
    Map<String, dynamic> patch,
  ) async {
    lastMetadataPatch = patch;
    return ProjectMetadata.fromJson(patch);
  }
}
