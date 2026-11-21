import 'package:composer_app/servers/server_api.dart';
import 'package:composer_app/state/server_entry.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('a server saved with a workspace path', () {
    test('is the server itself now: one home, no /w/<name>', () {
      expect(ServerEntry.withoutWorkspace('http://192.168.1.4:7420/w/default'), 'http://192.168.1.4:7420');
      expect(ServerEntry.withoutWorkspace('https://home.ts.net/w/work/'), 'https://home.ts.net');
    });

    test('any other URL stays as it is', () {
      expect(ServerEntry.withoutWorkspace('https://home.ts.net'), 'https://home.ts.net');
      expect(ServerEntry.withoutWorkspace('https://home.ts.net/composer'), 'https://home.ts.net/composer');
    });

    test('loading drops the path, keeping name and token', () {
      final entry = ServerEntry.fromJson({'url': 'http://h:7420/w/default', 'name': 'h', 'token': 't', 'workspace': 'default', 'color': ''});
      expect(entry.url, 'http://h:7420');
      expect(entry.toJson(), {'url': 'http://h:7420', 'name': 'h', 'token': 't'});
    });
  });

  group('pairing', () {
    test('the QR link gives the server URL', () {
      final parsed = parsePairInput('https://home.tail1234.ts.net/pair?code=AB12-CD34');
      expect(parsed.url, 'https://home.tail1234.ts.net');
      expect(parsed.code, 'AB12-CD34');
    });

    test('the composer login line too', () {
      final parsed = parsePairInput('composer login http://10.0.0.2:7420 AB12-CD34');
      expect(parsed.url, 'http://10.0.0.2:7420');
    });

    test('the WebSocket lives under it', () {
      expect(wsUri('https://home.ts.net').toString(), 'wss://home.ts.net/ws');
    });
  });
}
