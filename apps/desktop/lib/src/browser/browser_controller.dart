import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';

import '../core_client.dart';

/// Resolves `apps/browser-service` by walking up from [Directory.current].
String? resolveBrowserServiceDir({Directory? from}) {
  var dir = from ?? Directory.current;
  for (var i = 0; i < 8; i++) {
    final candidate = Directory(
      '${dir.path}${Platform.pathSeparator}apps'
      '${Platform.pathSeparator}browser-service',
    );
    if (candidate.existsSync()) return candidate.path;
    final parent = dir.parent;
    if (parent.path == dir.path) break;
    dir = parent;
  }
  return null;
}

String? _eventKind(Map<String, dynamic> event) {
  final nested = event['event'];
  final envelope = nested is Map
      ? nested.cast<String, dynamic>()
      : event;
  return envelope['kind']?.toString() ??
      envelope['name']?.toString() ??
      envelope['type']?.toString();
}

/// Thin controller over Core `browser.startService|stopService|call`.
class BrowserController extends ChangeNotifier {
  BrowserController(this.core, {String? serviceDir}) {
    _serviceDir = serviceDir ?? resolveBrowserServiceDir() ?? '';
    _events = core.events.listen(_onEvent);
  }

  final CoreClient core;
  StreamSubscription<Map<String, dynamic>>? _events;

  String _serviceDir = '';
  bool _running = false;
  bool _busy = false;
  String? _error;
  String? _lastResult;
  final List<String> _eventLog = [];

  String get serviceDir => _serviceDir;
  bool get running => _running;
  bool get busy => _busy;
  String? get error => _error;
  String? get lastResult => _lastResult;
  List<String> get eventLog => List.unmodifiable(_eventLog);

  void setServiceDir(String value) {
    _serviceDir = value.trim();
    notifyListeners();
  }

  Future<void> start() async {
    if (core.status != CoreConnectionStatus.connected || _busy) return;
    if (_serviceDir.isEmpty) {
      _error =
          'Set the browser-service directory (apps/browser-service) first.';
      notifyListeners();
      return;
    }
    _busy = true;
    _error = null;
    notifyListeners();
    try {
      final result = await core.request(
        'browser.startService',
        params: {'dir': _serviceDir},
      );
      _running = true;
      if (result['alreadyRunning'] == true) {
        _lastResult = 'Browser service already running.';
      } else {
        _lastResult = 'Browser service started.';
      }
      await refreshStatus();
    } catch (error) {
      _running = false;
      _error = error.toString();
    } finally {
      _busy = false;
      notifyListeners();
    }
  }

  Future<void> stop() async {
    if (core.status != CoreConnectionStatus.connected || _busy) return;
    _busy = true;
    _error = null;
    notifyListeners();
    try {
      await core.request('browser.stopService');
      _running = false;
      _lastResult = 'Browser service stopped.';
    } catch (error) {
      _error = error.toString();
    } finally {
      _busy = false;
      notifyListeners();
    }
  }

  Future<void> refreshStatus() async {
    if (core.status != CoreConnectionStatus.connected || !_running) return;
    try {
      final result = await core.request(
        'browser.call',
        params: {'method': 'browser.status', 'params': <String, dynamic>{}},
      );
      _lastResult = result.toString();
      _error = null;
    } catch (error) {
      _error = error.toString();
    }
    notifyListeners();
  }

  Future<void> navigate(String url) async {
    final trimmed = url.trim();
    if (trimmed.isEmpty ||
        core.status != CoreConnectionStatus.connected ||
        _busy) {
      return;
    }
    _busy = true;
    _error = null;
    notifyListeners();
    try {
      if (!_running) {
        await start();
        if (!_running) return;
      }
      final result = await core.request(
        'browser.call',
        params: {
          'method': 'browser.navigate',
          'params': {'url': trimmed},
        },
      );
      _lastResult = result.toString();
    } catch (error) {
      _error = error.toString();
    } finally {
      _busy = false;
      notifyListeners();
    }
  }

  void _onEvent(Map<String, dynamic> event) {
    final kind = _eventKind(event);
    if (kind == 'browser.serviceExited') {
      _running = false;
      _appendLog('service exited');
      notifyListeners();
      return;
    }
    if (kind == 'browser.event') {
      final payload = event['payload'] ?? event;
      _appendLog(payload.toString());
      notifyListeners();
    }
  }

  void _appendLog(String line) {
    _eventLog.insert(0, line);
    if (_eventLog.length > 40) {
      _eventLog.removeRange(40, _eventLog.length);
    }
  }

  @override
  void dispose() {
    unawaited(_events?.cancel());
    super.dispose();
  }
}
