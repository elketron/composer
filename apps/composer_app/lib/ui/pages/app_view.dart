import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_inappwebview/flutter_inappwebview.dart';

import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;

/// A site's micro app in the embedded browser, opened on Home with a token of its own: the server
/// answers `POST <url>/api/apps/<site>/open` with the URL to load (its token, or a client app's
/// one-time code, in it once), and that sets the cookie for this webview, so the app's API calls,
/// `send()` and live updates work as in a browser. The workspace token never goes in an app's URL.
class AppView extends StatefulWidget {
  const AppView({super.key, required this.url, required this.token, required this.site, this.session});

  final String url;
  final String token;
  final String site;

  /// The open chat's session: the app's `send()` goes there.
  final String? session;

  /// Asks the server to open the app: the address to load.
  Future<String> open() async {
    final base = Uri.parse(url);
    final client = HttpClient();
    try {
      final request = await client.postUrl(base.replace(path: '${base.path}/api/apps/$site/open'));
      request.headers.set(HttpHeaders.authorizationHeader, 'Bearer $token');
      final response = await request.close();
      final body = await response.transform(utf8.decoder).join();
      if (response.statusCode != 200) throw HttpException(body);
      final opened = jsonDecode(body)['url'] as String;
      final address = base.replace(path: '', query: null).toString().replaceAll(RegExp(r'[?#]$'), '');
      return '$address$opened${session == null ? '' : '&session=$session'}';
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
          initialSettings: InAppWebViewSettings(transparentBackground: false, isInspectable: true),
          onWebViewCreated: (controller) {
            // Linux beta: initialUrlRequest can be ignored (flutter_inappwebview#2903).
            if (Platform.isLinux) controller.loadUrl(urlRequest: URLRequest(url: WebUri(address)));
          },
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
