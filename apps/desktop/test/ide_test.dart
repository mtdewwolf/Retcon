import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_desktop/src/core_client.dart';
import 'package:retcon_desktop/src/ide/ide.dart';
import 'package:retcon_design_system/retcon_design_system.dart';

void main() {
  test('IDE feature contains no direct operating-system process launch', () {
    final source = Directory('lib/src/ide')
        .listSync(recursive: true)
        .whereType<File>()
        .map((file) => file.readAsStringSync())
        .join('\n');
    expect(source, isNot(contains('Process.start')));
    expect(source, isNot(contains('Process.run')));
    expect(source, isNot(contains('xdg-open')));
    expect(source, isNot(contains("'/c', 'start'")));
  });

  test(
    'Core IDE repository routes every launch through typed Core RPCs',
    () async {
      final core = _IdeCore();
      final repository = CoreIdeRepository(core);

      expect((await repository.detect()).map((ide) => ide.id), [
        'vscode',
        'cursor',
      ]);
      expect((await repository.configuration()).preferredIdeId, 'cursor');
      await repository.updatePreferred('vscode');
      await repository.openProject(r'C:\work', ideId: 'vscode');
      await repository.openWorktree(r'C:\work\tree', ideId: 'cursor');
      await repository.openFile(
        workspacePath: r'C:\work',
        path: r'C:\work\lib\main.dart',
        ideId: 'vscode',
        line: 12,
        column: 3,
      );
      await repository.openDiff(
        workspacePath: r'C:\work',
        leftPath: 'before.dart',
        rightPath: 'after.dart',
      );
      await repository.openTerminalLocation(
        workspacePath: r'C:\work',
        path: 'lib',
      );

      expect(
        core.calls.map((call) => call.$1),
        containsAllInOrder([
          'ide.detect',
          'ide.configuration.get',
          'ide.configuration.update',
          'ide.openProject',
          'ide.openWorktree',
          'ide.openFile',
          'ide.openDiff',
          'ide.openTerminalLocation',
        ]),
      );
      expect(core.calls.firstWhere((call) => call.$1 == 'ide.openFile').$2, {
        'workspacePath': r'C:\work',
        'path': r'C:\work\lib\main.dart',
        'ideId': 'vscode',
        'line': 12,
        'column': 3,
      });
      core.dispose();
    },
  );

  test(
    'revision conflict preserves the draft and supports both recovery choices',
    () async {
      final repository = _ConflictFiles();
      final controller = SyncedFileController(
        repository: repository,
        root: r'C:\work',
        path: 'note.txt',
      );
      addTearDown(controller.dispose);
      await controller.load();
      controller.edit('my draft');
      await controller.save();

      expect(controller.conflict, isTrue);
      expect(controller.draft, 'my draft');
      expect(controller.externalVersion?.content, 'external edit');
      expect(repository.writes.single.$4, 'rev-1');

      controller.keepDraftOnLatestRevision();
      expect(controller.draft, 'my draft');
      expect(controller.file?.revision, 'rev-2');
      expect(controller.dirty, isTrue);
      await controller.save();
      expect(repository.writes.last.$4, 'rev-2');
    },
  );

  test(
    'bounded file events refresh clean editors without replacing drafts',
    () async {
      final repository = _ConflictFiles();
      final events = StreamController<Map<String, dynamic>>.broadcast();
      final controller = SyncedFileController(
        repository: repository,
        root: r'C:\work',
        path: 'note.txt',
        events: events.stream,
      );
      addTearDown(() async {
        controller.dispose();
        await events.close();
      });
      await controller.load();
      events.add({
        'kind': 'file.changed',
        'payload': {'path': 'note.txt'},
      });
      await Future<void>.delayed(const Duration(milliseconds: 220));
      expect(controller.file?.revision, 'rev-2');

      controller.edit('unsaved draft');
      events.add({
        'kind': 'file.changed',
        'payload': {'path': 'note.txt'},
      });
      await Future<void>.delayed(const Duration(milliseconds: 220));
      expect(controller.draft, 'unsaved draft');
      expect(controller.externalChangePending, isTrue);
    },
  );

  testWidgets('editor explains a conflict without replacing typed content', (
    tester,
  ) async {
    final files = _ConflictFiles();
    final ide = IdeController(_FakeIdeRepository());
    await ide.load();
    addTearDown(ide.dispose);
    await tester.pumpWidget(
      MaterialApp(
        theme: buildLunaDarkTheme(),
        home: Scaffold(
          body: SizedBox(
            width: 900,
            height: 600,
            child: SyncedFileEditor(
              repository: files,
              ide: ide,
              root: r'C:\work',
              path: 'note.txt',
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    await tester.enterText(
      find.byKey(const Key('synced-file-editor')),
      'my safe draft',
    );
    await tester.pump();
    await tester.tap(find.byKey(const Key('synced-file-save')));
    await tester.pumpAndSettle();

    expect(find.textContaining('changed outside Retcon'), findsOneWidget);
    expect(find.text('Use external version'), findsOneWidget);
    expect(find.text('Keep my draft'), findsOneWidget);
    expect(find.text('my safe draft'), findsOneWidget);
  });
}

class _IdeCore extends CoreClient {
  final calls = <(String, Map<String, dynamic>)>[];
  @override
  CoreConnectionStatus get status => CoreConnectionStatus.connected;
  @override
  Future<Map<String, dynamic>> request(
    String method, {
    Map<String, dynamic> params = const {},
    Duration timeout = const Duration(seconds: 30),
  }) async {
    calls.add((method, params));
    if (method == 'ide.detect') {
      return const {
        'ides': [
          {'id': 'vscode', 'name': 'VS Code', 'available': true},
          {'id': 'cursor', 'name': 'Cursor', 'available': true},
          {'id': 'unknown', 'name': 'Unknown', 'available': true},
        ],
      };
    }
    if (method == 'ide.configuration.get' ||
        method == 'ide.configuration.update') {
      return {'preferredIdeId': params['preferredIdeId'] ?? 'cursor'};
    }
    return {
      'launched': true,
      'ideId': params['ideId'] ?? 'cursor',
      'action': method,
    };
  }
}

class _ConflictFiles implements SyncedFileRepository {
  int reads = 0;
  bool conflictOnce = true;
  final writes = <(String, String, String, String)>[];

  @override
  Future<SyncedFile> read({
    required String root,
    required String path,
    String? ifNoneMatch,
    int? limit,
  }) async {
    reads++;
    return SyncedFile(
      path: path,
      revision: reads == 1 ? 'rev-1' : 'rev-2',
      content: reads == 1 ? 'original' : 'external edit',
      size: 13,
      truncated: false,
      binary: false,
      language: 'plaintext',
    );
  }

  @override
  Future<SyncedWriteResult> write({
    required String root,
    required String path,
    required String content,
    required String ifMatch,
  }) async {
    writes.add((root, path, content, ifMatch));
    if (conflictOnce) {
      conflictOnce = false;
      throw const FileRevisionConflict();
    }
    return SyncedWriteResult(
      path: path,
      revision: 'rev-3',
      size: content.length,
    );
  }
}

class _FakeIdeRepository implements IdeRepository {
  @override
  Future<IdeConfiguration> configuration() async =>
      const IdeConfiguration(preferredIdeId: 'vscode');
  @override
  Future<List<IdeDescriptor>> detect() async => const [
    IdeDescriptor(id: 'vscode', name: 'VS Code', available: true),
  ];
  @override
  Future<IdeConfiguration> updatePreferred(String? ideId) async =>
      IdeConfiguration(preferredIdeId: ideId);
  IdeLaunchResult get _opened =>
      const IdeLaunchResult(launched: true, ideId: 'vscode', action: 'open');
  @override
  Future<IdeLaunchResult> openProject(String path, {String? ideId}) async =>
      _opened;
  @override
  Future<IdeLaunchResult> openWorktree(String path, {String? ideId}) async =>
      _opened;
  @override
  Future<IdeLaunchResult> openFile({
    required String workspacePath,
    required String path,
    String? ideId,
    int? line,
    int? column,
  }) async => _opened;
  @override
  Future<IdeLaunchResult> openDiff({
    required String workspacePath,
    required String leftPath,
    required String rightPath,
    String? ideId,
  }) async => _opened;
  @override
  Future<IdeLaunchResult> openTerminalLocation({
    required String workspacePath,
    String? path,
    String? ideId,
  }) async => _opened;
}
