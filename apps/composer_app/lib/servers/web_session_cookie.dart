import 'dart:convert';
import 'dart:io';

import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter_inappwebview/flutter_inappwebview.dart';

/// `POST <workspace>/api/web-session`: where the server makes a browser session of the caller's
/// kind (the device's).
Uri webSessionUri(String workspaceUrl) {
  final base = Uri.parse(workspaceUrl);
  final path = base.path.endsWith('/') ? base.path.substring(0, base.path.length - 1) : base.path;
  return base.replace(path: '$path/api/web-session', query: null, fragment: null);
}

/// The URL a session's cookie is stored for: the workspace's origin and the cookie's path.
Uri cookieUri(String workspaceUrl, WebSession session) {
  final base = Uri.parse(workspaceUrl);
  return base.replace(path: session.path, query: null, fragment: null);
}

/// Puts a new browser session of the device (asked for with its token) in the webviews' cookie
/// store, as the server's own cookie: a webview then loads the workspace's pages without the token
/// in its URL. The token never goes in a cookie or a URL.
Future<void> setWebSessionCookie(String workspaceUrl, String token) async {
  final client = HttpClient();
  final WebSession session;
  try {
    final request = await client.postUrl(webSessionUri(workspaceUrl));
    request.headers.set(HttpHeaders.authorizationHeader, 'Bearer $token');
    final response = await request.close();
    final body = await response.transform(utf8.decoder).join();
    if (response.statusCode != 200) throw HttpException('the server answered ${response.statusCode}');
    session = WebSession.fromJson(jsonDecode(body) as Map<String, Object?>);
  } finally {
    client.close();
  }
  final url = cookieUri(workspaceUrl, session);
  await CookieManager.instance().setCookie(
    url: WebUri.uri(url),
    name: session.cookieName,
    value: session.value,
    path: session.path,
    isSecure: url.scheme == 'https',
    isHttpOnly: true,
    sameSite: HTTPCookieSameSitePolicy.STRICT,
  );
}
