import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_terminal_view/retcon_terminal_view.dart';

class _FakeTerminalService implements TerminalService {
  _FakeTerminalService({this.shells = const []});

  final List<TerminalShell> shells;
  int nextId = 1;

  @override
  Future<void> input({required int terminalId, required String data}) async {}

  @override
  Future<void> kill({required int terminalId}) async {}

  @override
  Future<List<TerminalSessionSummary>> listRecent({int limit = 20}) async => [];

  @override
  Future<void> resize({
    required int terminalId,
    required int cols,
    required int rows,
  }) async {}

  @override
  Future<String> scrollback({required String sessionId}) async => '';

  @override
  Future<TerminalStartResult> start({
    required String shell,
    String? cwd,
    required int cols,
    required int rows,
  }) async {
    final id = nextId++;
    return TerminalStartResult(
      terminalId: id,
      sessionId: 'session-$id',
      shell: shell,
    );
  }

  @override
  Future<List<TerminalShell>> detectShells() async => shells;
}

void main() {
  test('controller creates a tab on initialize', () async {
    final controller = TerminalController(
      service: _FakeTerminalService(
        shells: const [TerminalShell(id: 'pwsh', path: 'pwsh.exe')],
      ),
    );
    await controller.initialize();
    expect(controller.tabs, hasLength(1));
    expect(controller.tabs.first.shell, 'pwsh.exe');
    controller.dispose();
  });

  test('controller appends terminal.output events', () async {
    final events = StreamController<Map<String, dynamic>>.broadcast();
    final controller = TerminalController(
      service: _FakeTerminalService(
        shells: const [TerminalShell(id: 'pwsh', path: 'pwsh.exe')],
      ),
      events: events.stream,
    );
    await controller.initialize();
    events.add({
      'kind': 'terminal.output',
      'payload': {'id': 1, 'data': 'hello'},
    });
    await Future<void>.delayed(Duration.zero);
    expect(controller.activeTab?.output, 'hello');
    await events.close();
    controller.dispose();
  });
}
