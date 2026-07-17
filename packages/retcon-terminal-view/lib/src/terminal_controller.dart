import 'dart:async';

import 'package:flutter/foundation.dart';

import 'terminal_service.dart';

class TerminalTab {
  TerminalTab({
    required this.terminalId,
    required this.sessionId,
    required this.title,
    required this.shell,
    this.output = '',
    this.exitCode,
    this.alive = true,
  });

  final int terminalId;
  final String sessionId;
  final String title;
  final String shell;
  String output;
  int? exitCode;
  bool alive;
}

/// Manages multiple terminal tabs and core RPC/event subscriptions.
class TerminalController extends ChangeNotifier {
  TerminalController({
    required TerminalService service,
    Stream<Map<String, dynamic>>? events,
  }) : _service = service {
    _events = events?.listen(_onEvent);
  }

  final TerminalService _service;
  StreamSubscription<Map<String, dynamic>>? _events;
  final List<TerminalTab> _tabs = [];
  int _activeIndex = 0;
  bool _loading = false;
  String? _error;
  int _cols = 120;
  int _rows = 30;

  List<TerminalTab> get tabs => List.unmodifiable(_tabs);
  int get activeIndex => _activeIndex;
  TerminalTab? get activeTab =>
      _tabs.isEmpty ? null : _tabs[_activeIndex.clamp(0, _tabs.length - 1)];
  bool get loading => _loading;
  String? get error => _error;

  Future<void> initialize({String? cwd}) async {
    _loading = true;
    _error = null;
    notifyListeners();
    try {
      final recent = await _service.listRecent(limit: 8);
      for (final session in recent) {
        if (session.status == 'ended' || session.status == 'interrupted') {
          final text = await _service.scrollback(sessionId: session.sessionId);
          if (text.isEmpty) continue;
          _tabs.add(
            TerminalTab(
              terminalId: -1,
              sessionId: session.sessionId,
              title: _titleForShell(session.shell),
              shell: session.shell,
              output: text,
              alive: false,
            ),
          );
        }
      }
      if (_tabs.isEmpty) {
        await createTab(cwd: cwd);
      } else {
        _activeIndex = 0;
      }
    } on Object catch (error) {
      _error = error.toString();
    } finally {
      _loading = false;
      notifyListeners();
    }
  }

  Future<void> createTab({String? cwd}) async {
    _error = null;
    final shells = await _service.detectShells();
    final shell = shells.isNotEmpty ? shells.first.path : 'powershell.exe';
    final started = await _service.start(
      shell: shell,
      cwd: cwd,
      cols: _cols,
      rows: _rows,
    );
    _tabs.add(
      TerminalTab(
        terminalId: started.terminalId,
        sessionId: started.sessionId,
        title: _titleForShell(started.shell),
        shell: started.shell,
      ),
    );
    _activeIndex = _tabs.length - 1;
    notifyListeners();
  }

  void selectTab(int index) {
    if (index < 0 || index >= _tabs.length) return;
    _activeIndex = index;
    notifyListeners();
  }

  Future<void> closeTab(int index) async {
    if (index < 0 || index >= _tabs.length) return;
    final tab = _tabs[index];
    if (tab.alive && tab.terminalId >= 0) {
      await _service.kill(terminalId: tab.terminalId);
    }
    _tabs.removeAt(index);
    if (_tabs.isEmpty) {
      _activeIndex = 0;
    } else if (_activeIndex >= _tabs.length) {
      _activeIndex = _tabs.length - 1;
    }
    notifyListeners();
  }

  Future<void> sendInput(String data) async {
    final tab = activeTab;
    if (tab == null || !tab.alive || tab.terminalId < 0) return;
    await _service.input(terminalId: tab.terminalId, data: data);
  }

  Future<void> resize(int cols, int rows) async {
    _cols = cols.clamp(20, 500);
    _rows = rows.clamp(5, 200);
    final tab = activeTab;
    if (tab == null || !tab.alive || tab.terminalId < 0) return;
    await _service.resize(
      terminalId: tab.terminalId,
      cols: _cols,
      rows: _rows,
    );
  }

  void _onEvent(Map<String, dynamic> event) {
    final name =
        event['name']?.toString() ??
        event['kind']?.toString() ??
        event['type']?.toString() ??
        '';
    final payload =
        (event['payload'] as Map?)?.cast<String, dynamic>() ??
        (event['data'] as Map?)?.cast<String, dynamic>() ??
        event;
    if (name == 'terminal.output') {
      final id = (payload['id'] as num?)?.toInt();
      final data = payload['data']?.toString() ?? '';
      final tab = _tabs.where((item) => item.terminalId == id).firstOrNull;
      if (tab == null) return;
      tab.output += data;
      notifyListeners();
      return;
    }
    if (name == 'terminal.exit') {
      final id = (payload['id'] as num?)?.toInt();
      final code = (payload['exitCode'] as num?)?.toInt();
      final tab = _tabs.where((item) => item.terminalId == id).firstOrNull;
      if (tab == null) return;
      tab.alive = false;
      tab.exitCode = code;
      notifyListeners();
    }
  }

  static String _titleForShell(String shell) {
    final base = shell.split(RegExp(r'[\\/]')).last;
    return base.isEmpty ? 'Terminal' : base;
  }

  @override
  void dispose() {
    unawaited(_events?.cancel());
    super.dispose();
  }
}

extension<T> on Iterable<T> {
  T? get firstOrNull {
    final iterator = this.iterator;
    if (!iterator.moveNext()) return null;
    return iterator.current;
  }
}
