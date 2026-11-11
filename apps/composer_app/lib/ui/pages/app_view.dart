import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_inappwebview/flutter_inappwebview.dart';

import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;

/// A site's micro app in the embedded browser, opened with a token of its own: the server answers
/// `POST <url>/api/apps/<site>/open` with the URL to load, `<base>/a/<app token>/` (the app runs
/// sandboxed there, its API under that path), or for a client app a URL with a one-time code that
/// sets the cookie for this webview. The workspace token never goes in an app's URL.
class AppView extends StatefulWidget {
  const AppView({super.key, required this.url, required this.token, required this.site, this.session, this.project});

  final String url;
  final String token;
  final String site;

  /// The open chat's session: the app's `send()` goes there.
  final String? session;

  /// The project the app is opened on (a desk app tab); null: the workspace's Home.
  final String? project;

  /// Asks the server to open the app: the address to load.
  Future<String> open() async {
    final base = Uri.parse(url);
    final client = HttpClient();
    try {
      // Opened from the chat's session: the one the app's messages go to.
      final path = project == null
          ? '${base.path}/api/apps/$site/open'
          : '${base.path}/api/projects/$project/apps/$site/open';
      final open = base.replace(
        path: path,
        queryParameters: session == null ? null : {'session': session},
      );
      final request = await client.postUrl(open);
      request.headers.set(HttpHeaders.authorizationHeader, 'Bearer $token');
      final response = await request.close();
      final body = await response.transform(utf8.decoder).join();
      if (response.statusCode != 200) throw HttpException(body);
      final opened = jsonDecode(body)['url'] as String;
      final address = base.replace(path: '', query: null).toString().replaceAll(RegExp(r'[?#]$'), '');
      final join = opened.contains('?') ? '&' : '?';
      return '$address$opened${session == null ? '' : '${join}session=$session'}';
    } finally {
      client.close();
    }
  }

  @override
  State<AppView> createState() => _AppViewState();
}

class _AppViewState extends State<AppView> {
  String? _error;
  bool _loading = true;
  String? _address;

  @override
  void initState() {
    super.initState();
    widget.open().then(
          (address) => mounted ? setState(() => _address = address) : null,
          onError: (Object e) => mounted ? setState(() => _error = '$e') : null,
        );
  }

  @override
  Widget build(BuildContext context) => Stack(children: [
        if (_address case final address?)
        InAppWebView(
          key: ValueKey(address),
          initialUrlRequest: URLRequest(url: WebUri(address)),
          initialSettings: InAppWebViewSettings(
            transparentBackground: false,
            // Debuggable (chrome://inspect, Safari Web Inspector) in debug builds only.
            isInspectable: kDebugMode,
            // The page is the server's: no local files, no content:// providers, no cross-site
            // cookies, and an https page never loads http subresources (the page itself may be
            // http: a tailnet or emulator server).
            allowFileAccess: false,
            allowContentAccess: false,
            allowFileAccessFromFileURLs: false,
            allowUniversalAccessFromFileURLs: false,
            thirdPartyCookiesEnabled: false,
            mixedContentMode: MixedContentMode.MIXED_CONTENT_NEVER_ALLOW,
          ),
          onLoadStop: (_, _) => setState(() => _loading = false),
          onReceivedError: (_, request, error) {
            if (request.isForMainFrame ?? true) setState(() => _error = error.description);
          },
        ),
        if (_loading) const LinearProgressIndicator(minHeight: 2, color: t.accent, backgroundColor: Colors.transparent),
        if (_error != null)
          Container(
            color: t.bg,
            alignment: Alignment.center,
            padding: const EdgeInsets.all(24),
            child: Text("The app didn't load: $_error", style: mono(size: 12, color: t.err)),
          ),
      ]);
}
