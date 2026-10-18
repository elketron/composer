import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_inappwebview/flutter_inappwebview.dart';

import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;

/// A site's micro app in the embedded browser. The `?token=` sign-in sets the server's cookie
/// for this webview, so the app's API calls, `send()` and live updates work as in a browser.
class AppView extends StatefulWidget {
  const AppView({super.key, required this.url, required this.token, required this.site, this.session});

  final String url;
  final String token;
  final String site;

  /// The open chat's session: the app's `send()` goes there.
  final String? session;

  String get address =>
      '$url/$site/?token=${Uri.encodeQueryComponent(token)}${session == null ? '' : '&session=$session'}';

  @override
  State<AppView> createState() => _AppViewState();
}

class _AppViewState extends State<AppView> {
  String? _error;
  bool _loading = true;

  @override
  Widget build(BuildContext context) => Stack(children: [
        InAppWebView(
          key: ValueKey(widget.address),
          initialUrlRequest: URLRequest(url: WebUri(widget.address)),
          initialSettings: InAppWebViewSettings(transparentBackground: false, isInspectable: true),
          onWebViewCreated: (controller) {
            // Linux beta: initialUrlRequest can be ignored (flutter_inappwebview#2903).
            if (Platform.isLinux) controller.loadUrl(urlRequest: URLRequest(url: WebUri(widget.address)));
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
