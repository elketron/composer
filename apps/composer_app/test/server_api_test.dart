import 'package:composer_app/servers/server_api.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('the QR link of composer pair', () {
    final parsed = parsePairInput('https://home.tail1234.ts.net/pair?code=AB12-CD34');
    expect(parsed.url, 'https://home.tail1234.ts.net');
    expect(parsed.code, 'AB12-CD34');
  });

  test('the composer login line', () {
    final parsed = parsePairInput('  composer login https://home.ts.net/ AB12-CD34\n');
    expect(parsed.url, 'https://home.ts.net');
    expect(parsed.code, 'AB12-CD34');
  });

  test('a bare address', () {
    expect(parsePairInput('home.ts.net').url, 'https://home.ts.net');
    expect(parsePairInput('home.ts.net').code, isNull);
    expect(parsePairInput('localhost:7420').url, 'http://localhost:7420');
    final both = parsePairInput('http://10.0.2.2:7420 AB12-CD34');
    expect((both.url, both.code), ('http://10.0.2.2:7420', 'AB12-CD34'));
    // Half typed: no exception, nothing to pair with yet.
    expect(parsePairInput('http://10.0.2.2:74b').code, isNull);
    expect(parsePairInput('http://127.0.0.1:7420/').url, 'http://127.0.0.1:7420');
  });

  test('websocket address', () {
    expect(wsUri('https://home.ts.net').toString(), 'wss://home.ts.net/ws');
    expect(wsUri('http://127.0.0.1:7420').toString(), 'ws://127.0.0.1:7420/ws');
  });
}
