import 'package:composer_app/state/server_entry.dart';
import 'package:composer_app/state/server_group.dart';
import 'package:composer_app/state/server_mark.dart';
import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter_test/flutter_test.dart';

ServerInfo info() => const ServerInfo(name: 'reported', version: '0.4.0', model: 'm', slots: 2);

ServerEntry entry(String url) => ServerEntry(url: url, name: 'home', token: 't');

void main() {
  test('a colour from a name is the shared derivation', () {
    // FNV-1a, hsl(hash % 360, .55, .62).
    expect(ServerColor.fromName('default').hex, '#69d3c9');
    expect(ServerColor.fromName('work').hex, '#9369d3');
    expect(ServerColor.fromName('aaa').hex, '#d37b69');
  });

  group('a server\'s mark', () {
    test('is its saved name, in that name\'s colour', () {
      final m = ServerMark.of('https://home.ts.net', info: info(), entry: entry('https://home.ts.net'));
      expect(m.name, 'home');
      expect(m.color, ServerColor.fromName('home'));
    });

    test('falls back to the name it reports, then its host', () {
      expect(ServerMark.of('https://x.ts.net', info: info()).name, 'reported');
      expect(ServerMark.of('https://x.ts.net').name, 'x.ts.net');
    });
  });

  group('grouping', () {
    test('servers are grouped by host', () {
      final groups = ServerGroup.group([
        entry('https://home.ts.net'),
        entry('https://other.ts.net'),
      ]);
      expect(groups.map((g) => g.host), ['home.ts.net', 'other.ts.net']);
    });

    test('a host with a port is its own group', () {
      final groups = ServerGroup.group([
        entry('http://127.0.0.1:7420'),
        entry('http://127.0.0.1:7531'),
      ]);
      expect(groups.map((g) => g.host), ['127.0.0.1:7420', '127.0.0.1:7531']);
    });
  });
}
