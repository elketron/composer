import 'package:composer_app/servers/web_session_cookie.dart';
import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('the session is asked of the workspace and stored for its path', () {
    expect(webSessionUri('https://h.ts.net/w/default').toString(), 'https://h.ts.net/w/default/api/web-session');
    expect(webSessionUri('http://10.0.2.2:7420/').toString(), 'http://10.0.2.2:7420/api/web-session');
    const session = WebSession(cookieName: 'composer_token', value: 'ses_x', path: '/w/default/');
    expect(cookieUri('https://h.ts.net/w/default', session).toString(), 'https://h.ts.net/w/default/');
    const alone = WebSession(cookieName: 'composer_token_7420', value: 'ses_x', path: '/');
    expect(cookieUri('http://10.0.2.2:7420', alone).toString(), 'http://10.0.2.2:7420/');
  });
}
