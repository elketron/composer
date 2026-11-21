import 'package:composer_app/state/server_entry.dart';
import 'package:composer_app/state/workspace.dart';
import 'package:composer_app/state/workspace_color.dart';
import 'package:composer_app/state/workspace_group.dart';
import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter_test/flutter_test.dart';

ServerInfo info() => const ServerInfo(name: 'home', version: '0.4.0', model: 'm', slots: 2);

ServerEntry entry(String url) => ServerEntry(url: url, name: 'home', token: 't');

void main() {
  group('workspace colour', () {
    test('parses #rrggbb, rejects anything else', () {
      expect(WorkspaceColor.parse('#ff8800')!.hex, '#ff8800');
      expect(WorkspaceColor.parse('ff8800')!.value, 0xff8800);
      expect(WorkspaceColor.parse('#fff'), isNull);
      expect(WorkspaceColor.parse('#gggggg'), isNull);
      expect(WorkspaceColor.parse(''), isNull);
      expect(WorkspaceColor.parse(null), isNull);
    });

    test('from a name is the server\'s derivation', () {
      // composer_protocol::WorkspaceColor::from_name: FNV-1a, hsl(hash % 360, .55, .62).
      expect(WorkspaceColor.fromName('default').hex, '#69d3c9');
      expect(WorkspaceColor.fromName('work').hex, '#9369d3');
      expect(WorkspaceColor.fromName('aaa').hex, '#d37b69');
    });
  });

  test('a server has one home: default, in its colour', () {
    final w = Workspace.of('https://home.ts.net', info: info(), entry: entry('https://home.ts.net'));
    expect(w.name, 'default');
    expect(w.color, WorkspaceColor.fromName('default'));
  });

  group('grouping', () {
    test('servers are grouped by host', () {
      final groups = WorkspaceGroup.group([
        entry('https://home.ts.net'),
        entry('https://other.ts.net'),
      ]);
      expect(groups.map((g) => g.host), ['home.ts.net', 'other.ts.net']);
    });

    test('a host with a port is its own group', () {
      final groups = WorkspaceGroup.group([
        entry('http://127.0.0.1:7420'),
        entry('http://127.0.0.1:7531'),
      ]);
      expect(groups.map((g) => g.host), ['127.0.0.1:7420', '127.0.0.1:7531']);
    });
  });
}
