import 'package:composer_app/state/server_entry.dart';
import 'package:composer_app/state/workspace.dart';
import 'package:composer_app/state/workspace_color.dart';
import 'package:composer_app/state/workspace_group.dart';
import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter_test/flutter_test.dart';

ServerInfo info({String? workspace, String? color}) => ServerInfo(
      name: 'home',
      version: '0.4.0',
      model: 'm',
      slots: 2,
      workspace: workspace,
      workspaceColor: color,
    );

ServerEntry entry(String url, {String workspace = '', String color = ''}) =>
    ServerEntry(url: url, name: 'home', token: 't', workspace: workspace, color: color);

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

  group('a workspace as shown', () {
    test('comes from the server when it says', () {
      final w = Workspace.of('https://home.ts.net/w/work', info: info(workspace: 'work', color: '#ff8800'));
      expect(w.name, 'work');
      expect(w.color.hex, '#ff8800');
    });

    test('falls back to the URL name and its colour on an old server', () {
      final w = Workspace.of('https://home.ts.net/w/work', info: info());
      expect(w.name, 'work');
      expect(w.color, WorkspaceColor.fromName('work'));
    });

    test('prefers the server over the saved entry, then the entry', () {
      final saved = entry('https://home.ts.net/w/work', workspace: 'saved', color: '#123456');
      final fromServer = Workspace.of(saved.url, info: info(workspace: 'work', color: '#ff8800'), entry: saved);
      expect(fromServer.name, 'work');
      expect(fromServer.color.hex, '#ff8800');
      final offline = Workspace.of(saved.url, info: null, entry: saved);
      expect(offline.name, 'saved');
      expect(offline.color.hex, '#123456');
    });
  });

  group('grouping', () {
    test('two workspaces of one host share a group', () {
      final groups = WorkspaceGroup.group([
        entry('https://home.ts.net/w/default'),
        entry('https://other.ts.net/w/default'),
        entry('https://home.ts.net/w/work'),
      ]);
      expect(groups.map((g) => g.host), ['home.ts.net', 'other.ts.net']);
      expect(groups.first.entries.map((e) => e.url), [
        'https://home.ts.net/w/default',
        'https://home.ts.net/w/work',
      ]);
    });

    test('a host with a port is its own group', () {
      final groups = WorkspaceGroup.group([
        entry('http://127.0.0.1:7420/w/default'),
        entry('http://127.0.0.1:7531/w/default'),
      ]);
      expect(groups.map((g) => g.host), ['127.0.0.1:7420', '127.0.0.1:7531']);
    });
  });
}
