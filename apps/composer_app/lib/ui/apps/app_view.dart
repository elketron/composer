import 'package:flutter/foundation.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_inappwebview/flutter_inappwebview.dart';
import 'package:url_launcher/url_launcher.dart';

import '../../servers/web_session_cookie.dart';
import '../../state/apps.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;

/// Why an app's page did not load, in words: the proxy's 502/503/504 means the app is not running
/// (composer could not reach it); anything else is said as it came.
({String headline, String detail}) appFailure(String title, {int? status, String? error}) => switch (status) {
      502 || 503 || 504 => (
          headline: '$title is not running',
          detail: 'composer could not reach it (the server answered $status). Start it, then reload.',
        ),
      final int code => (headline: '$title could not load', detail: 'the server answered $code'),
      null => (headline: '$title could not load', detail: error ?? 'unknown error'),
    };

/// One of the user's local apps (`/apps/<name>/<path>` on the workspace) in a webview, with
/// JavaScript, its iframes, event streams and WebSockets, which the server forwards. It
/// authenticates as the live view does: a browser session of the device in the webview's cookie,
/// asked for before the first load (and once more when the page answers 401/404), never with the
/// token in the URL. Links inside `/apps/<name>/` stay here; any other the page navigates to opens
/// outside (the browser), as a page's external links do. When the page cannot load (the app is
/// down: the proxy's 502), a readable state with Reload stands in.
class AppView extends StatefulWidget {
  const AppView({
    super.key,
    required this.workspace,
    required this.token,
    required this.name,
    required this.title,
    required this.path,
    this.reloads,
  });

  final String workspace;
  final String token;
  final String name;
  final String title;

  /// Relative to the app's root; a new one (the agent moved the tab) loads it.
  final String path;

  /// A tick of it reloads the page (the screen's Reload).
  final Listenable? reloads;

  @override
  State<AppView> createState() => _AppViewState();
}

class _AppViewState extends State<AppView> {
  InAppWebViewController? _controller;
  ({String headline, String detail})? _failure;
  bool _loading = true;

  /// Bumped by Reload after a failure: a new webview.
  int _attempt = 0;

  /// The session cookie is in place (the webview is made only then).
  bool _ready = false;

  /// The session was renewed once after a 401/404 since the last (re)try.
  bool _renewed = false;

  String get _address => appUrl(widget.workspace, widget.name, widget.path);

  @override
  void initState() {
    super.initState();
    widget.reloads?.addListener(_reload);
    _session();
  }

  @override
  void dispose() {
    widget.reloads?.removeListener(_reload);
    super.dispose();
  }

  @override
  void didUpdateWidget(AppView old) {
    super.didUpdateWidget(old);
    if (old.reloads != widget.reloads) {
      old.reloads?.removeListener(_reload);
      widget.reloads?.addListener(_reload);
    }
    if (old.path != widget.path || old.name != widget.name) {
      if (_failure != null) {
        _reload();
      } else {
        _controller?.loadUrl(urlRequest: URLRequest(url: WebUri(_address)));
      }
    }
  }

  Future<void> _session() async {
    try {
      await setWebSessionCookie(widget.workspace, widget.token);
      if (mounted) setState(() => _ready = true);
    } catch (e) {
      _failed(error: 'no browser session: $e');
    }
  }

  /// After a failure: a new webview (and session); otherwise the page again.
  void _reload() {
    if (!mounted) return;
    if (_failure == null && _controller != null) {
      setState(() => _loading = true);
      _controller!.reload();
      return;
    }
    setState(() {
      _failure = null;
      _loading = true;
      _ready = false;
      _renewed = false;
      _attempt++;
    });
    _session();
  }

  Future<void> _renew(int status) async {
    if (_renewed) return _failed(status: status);
    _renewed = true;
    try {
      await setWebSessionCookie(widget.workspace, widget.token);
      await _controller?.reload();
    } catch (e) {
      _failed(error: 'no browser session: $e');
    }
  }

  void _failed({int? status, String? error}) {
    if (!mounted) return;
    setState(() => _failure = appFailure(widget.title, status: status, error: error));
  }

  Future<NavigationActionPolicy> _navigate(NavigationAction action) async {
    final target = action.request.url;
    // Frames inside the page load what the page puts in them.
    if (target == null || !action.isForMainFrame) return NavigationActionPolicy.ALLOW;
    if (insideApp(widget.workspace, widget.name, target.uriValue)) return NavigationActionPolicy.ALLOW;
    if (target.scheme == 'about' || target.scheme == 'data' || target.scheme == 'blob') return NavigationActionPolicy.ALLOW;
    launchUrl(target.uriValue, mode: LaunchMode.externalApplication);
    return NavigationActionPolicy.CANCEL;
  }

  @override
  Widget build(BuildContext context) {
    final failure = _failure;
    return ColoredBox(
      color: t.surface,
      child: Stack(children: [
        if (failure == null && _ready)
          Positioned.fill(
            child: InAppWebView(
              key: ValueKey('app-view-$_attempt'),
              initialUrlRequest: URLRequest(url: WebUri(_address)),
              initialSettings: InAppWebViewSettings(
                javaScriptEnabled: true,
                domStorageEnabled: true,
                useShouldOverrideUrlLoading: true,
                transparentBackground: false,
                isInspectable: kDebugMode,
                // The app is the user's own, through the server: no local files, no content://
                // providers, no cross-site cookies; an https page never loads http subresources.
                allowFileAccess: false,
                allowContentAccess: false,
                allowFileAccessFromFileURLs: false,
                allowUniversalAccessFromFileURLs: false,
                thirdPartyCookiesEnabled: false,
                mixedContentMode: MixedContentMode.MIXED_CONTENT_NEVER_ALLOW,
                // One page: a link that wants a new window opens in this one (or outside).
                supportMultipleWindows: false,
              ),
              // An app such as a canvas pans and pinches itself: its touches stay with it.
              gestureRecognizers: {Factory<OneSequenceGestureRecognizer>(EagerGestureRecognizer.new)},
              onWebViewCreated: (controller) => _controller = controller,
              shouldOverrideUrlLoading: (_, action) => _navigate(action),
              onLoadStop: (_, _) {
                if (mounted) setState(() => _loading = false);
              },
              onReceivedError: (_, request, error) {
                if (request.isForMainFrame ?? true) _failed(error: error.description);
              },
              onReceivedHttpError: (_, request, response) {
                if (!(request.isForMainFrame ?? true)) return;
                final status = response.statusCode ?? 0;
                if (status == 401 || status == 404) {
                  _renew(status);
                } else {
                  _failed(status: status);
                }
              },
            ),
          ),
        if (_loading && failure == null)
          const Positioned(left: 0, right: 0, top: 0, child: LinearProgressIndicator(minHeight: 2, color: t.accent, backgroundColor: Colors.transparent)),
        if (failure != null) Positioned.fill(child: AppUnavailable(headline: failure.headline, detail: failure.detail, onReload: _reload)),
      ]),
    );
  }
}

/// What stands in for an app that did not load: what happened, and Reload.
class AppUnavailable extends StatelessWidget {
  const AppUnavailable({super.key, required this.headline, required this.detail, required this.onReload});

  final String headline;
  final String detail;
  final VoidCallback onReload;

  @override
  Widget build(BuildContext context) => Center(
        key: const Key('app-unavailable'),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 24),
          child: Column(mainAxisSize: MainAxisSize.min, children: [
            Text(headline, textAlign: TextAlign.center, style: mono(size: 13, color: t.text, weight: FontWeight.w600)),
            const SizedBox(height: 8),
            Text(detail, textAlign: TextAlign.center, style: mono(size: 11.5, color: t.muted)),
            const SizedBox(height: 16),
            OutlinedButton.icon(
              key: const Key('app-reload'),
              style: OutlinedButton.styleFrom(minimumSize: const Size(96, 44)),
              onPressed: onReload,
              icon: const Icon(Icons.refresh, size: 18),
              label: const Text('Reload'),
            ),
          ]),
        ),
      );
}
