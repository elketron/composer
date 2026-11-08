import 'package:composer_app/servers/server_api.dart';
import 'package:composer_app/state/server_entry.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('a server saved before workspaces', () {
    test('is its default workspace now', () {
      expect(ServerEntry.inWorkspace('http://192.168.1.4:7420'), 'http://192.168.1.4:7420/w/default');
      expect(ServerEntry.inWorkspace('https://home.ts.net/'), 'https://home.ts.net/w/default');
    });

    test('a workspace URL stays as it is', () {
      expect(ServerEntry.inWorkspace('https://home.ts.net/w/b'), 'https://home.ts.net/w/b');
    });

    test('loading moves it, keeping name and token', () {
      final entry = ServerEntry.fromJson({'url': 'http://h:7420', 'name': 'h', 'token': 't'});
      expect(entry.url, 'http://h:7420/w/default');
      expect(entry.workspace, '', reason: 'unknown until the server answers');
      expect(entry.toJson(), {
        'url': 'http://h:7420/w/default',
        'name': 'h',
        'token': 't',
        'workspace': '',
        'color': '',
      });
    });
  });

  group('the workspace in a URL', () {
    test('is its /w/<name>, else default', () {
      expect(ServerEntry.workspaceIn('https://home.ts.net/w/work'), 'work');
      expect(ServerEntry.workspaceIn('https://home.ts.net/w/work/'), 'work');
      expect(ServerEntry.workspaceIn('https://home.ts.net/'), 'default');
      expect(ServerEntry.workspaceIn('http://h:7420'), 'default');
    });

    test('is kept by a saved entry', () {
      final entry = ServerEntry.fromJson({
        'url': 'https://home.ts.net/w/work',
        'name': 'home',
        'token': 't',
        'workspace': 'work',
        'color': '#ff8800',
      });
      expect(entry.workspace, 'work');
      expect(entry.color, '#ff8800');
      expect(ServerEntry.fromJson(entry.toJson()).color, '#ff8800');
    });
  });

  group('pairing with a workspace', () {
    test('the QR link keeps the workspace in the base URL', () {
      final parsed = parsePairInput('https://home.tail1234.ts.net/w/b/pair?code=AB12-CD34');
      expect(parsed.url, 'https://home.tail1234.ts.net/w/b');
      expect(parsed.code, 'AB12-CD34');
    });

    test('the composer login line too', () {
      final parsed = parsePairInput('composer login http://10.0.0.2:7420/w/default AB12-CD34');
      expect(parsed.url, 'http://10.0.0.2:7420/w/default');
    });

    test('the WebSocket and API live under it', () {
      expect(wsUri('https://home.ts.net/w/b').toString(), 'wss://home.ts.net/w/b/ws');
    });
  });
}
