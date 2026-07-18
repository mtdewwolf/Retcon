import 'dart:io';

import 'package:path/path.dart' as p;

/// Resolve the Retcon repository root from env or by walking parents.
String? findRepoRoot([String? start]) {
  final fromEnv = Platform.environment['RETCON_ROOT'];
  if (fromEnv != null && _looksLikeRepo(fromEnv)) {
    return p.normalize(fromEnv);
  }

  var current = p.normalize(start ?? Directory.current.path);
  while (true) {
    if (_looksLikeRepo(current)) {
      return current;
    }
    final parent = p.dirname(current);
    if (parent == current) {
      return null;
    }
    current = parent;
  }
}

bool _looksLikeRepo(String path) {
  final root = Directory(path);
  if (!root.existsSync()) return false;
  return File(p.join(path, 'Cargo.toml')).existsSync() &&
      Directory(p.join(path, 'apps', 'desktop')).existsSync();
}
