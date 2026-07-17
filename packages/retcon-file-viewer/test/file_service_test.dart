import 'package:flutter_test/flutter_test.dart';
import 'package:retcon_file_viewer/retcon_file_viewer.dart';

void main() {
  test('RpcFileService maps list entries', () async {
    final service = RpcFileService((method, {params = const {}}) async {
      expect(method, 'file.list');
      return {
        'entries': [
          {
            'name': 'README.md',
            'path': '/tmp/README.md',
            'isDirectory': false,
            'size': 12,
            'gitStatus': 'modified',
          },
        ],
      };
    });

    final entries = await service.list(root: '/tmp');
    expect(entries, hasLength(1));
    expect(entries.first.name, 'README.md');
    expect(entries.first.gitStatus, 'modified');
  });

  test('FakeFileService returns configured read result', () async {
    final service = FakeFileService(
      readResult: FileReadResult(
        path: 'lib/main.dart',
        content: 'void main() {}',
        size: 14,
        truncated: false,
        binary: false,
        language: 'dart',
      ),
    );

    final read = await service.read(root: '/project', path: 'lib/main.dart');
    expect(read.language, 'dart');
    expect(read.content, 'void main() {}');
  });
}
