import 'dart:collection';

import 'package:flutter/foundation.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_inappwebview/flutter_inappwebview.dart';

import '../../servers/web_session_cookie.dart';
import '../../state/frame_size.dart';
import '../../state/live_view_size.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;

/// The application's screen as the viewer serves it (`?bare=1`), view only and updating live. The
/// page zooms and pans itself (pinch, drag, double tap) and fits the screen to the box as the
/// size says (`window.wayaiBare.setFit`); it tells the size of the screen through
/// `window.wayaiBare.frame` and the `wayai-frame` event, which a script injected here hands to
/// [onFrameSize]. Nothing is drawn over the picture: the name and "view only" live in the handle
/// row. When the page cannot load, a compact inline state with Retry stands in for the webview.
/// Edge to edge. It authenticates with a browser session of the device in the webview's cookie,
/// asked for before the first load (and once more when the page answers 401/404: the session
/// expired or the server restarted), never with the token in the URL.
class LiveView extends StatefulWidget {
  const LiveView({
    super.key,
    required this.address,
    required this.workspace,
    required this.token,
    required this.size,
    required this.onFrameSize,
    required this.onLive,
  });

  /// The bare viewer page (no token in it).
  final String address;

  /// The workspace's URL and the device token: what the browser session is asked for with.
  final String workspace;
  final String token;
  final LiveViewSize size;
  final ValueChanged<FrameSize> onFrameSize;
  final ValueChanged<bool> onLive;

  @override
  State<LiveView> createState() => _LiveViewState();
}

class _LiveViewState extends State<LiveView> {
  static const _frameHandler = 'wayaiFrame';

  /// Runs before the page's own script: hands every `wayai-frame` event to the app.
  static final _frameScript = UserScript(
    source: '''
window.addEventListener("wayai-frame", function (event) {
  var bridge = window.flutter_inappwebview;
  if (bridge) bridge.callHandler("$_frameHandler", event.detail.width, event.detail.height);
});
''',
    injectionTime: UserScriptInjectionTime.AT_DOCUMENT_START,
  );

  InAppWebViewController? _controller;
  String? _error;
  bool _loading = true;

  /// Bumped by Retry: a new webview.
  int _attempt = 0;

  /// The session cookie is in place (the webview is made only then).
  bool _ready = false;

  /// The session was renewed once after a 401/404 since the last (re)try.
  bool _renewed = false;

  @override
  void initState() {
    super.initState();
    _session();
  }

  /// Asks for a browser session and stores it; then the webview loads.
  Future<void> _session() async {
    try {
      await setWebSessionCookie(widget.workspace, widget.token);
      if (mounted) setState(() => _ready = true);
    } catch (e) {
      _failed('no browser session: $e');
    }
  }

  void _retry() {
    widget.onLive(true);
    setState(() {
      _error = null;
      _loading = true;
      _ready = false;
      _renewed = false;
      _attempt++;
    });
    _session();
  }

  /// The page answered 401/404: the session may be gone. A new one and the page again, once.
  Future<void> _renew(int status) async {
    if (_renewed) return _failed('the server answered $status');
    _renewed = true;
    try {
      await setWebSessionCookie(widget.workspace, widget.token);
      await _controller?.reload();
    } catch (e) {
      _failed('no browser session: $e');
    }
  }

  void _failed(String why) {
    if (!mounted) return;
    widget.onLive(false);
    setState(() => _error = why);
  }

  void _report(List<dynamic> args) {
    final frame = FrameSize.tryFrom(args.elementAtOrNull(0), args.elementAtOrNull(1));
    if (frame != null && mounted) widget.onFrameSize(frame);
  }

  Future<void> _setFit() =>
      _controller?.evaluateJavascript(source: 'window.wayaiBare && window.wayaiBare.setFit("${widget.size.fit}")') ?? Future.value();

  /// The page may have shown its first frame before the handler could hear it: ask once.
  Future<void> _readFrame() async {
    final size = await _controller?.evaluateJavascript(source: 'window.wayaiBare && window.wayaiBare.frame');
    if (size is Map) _report([size['width'], size['height']]);
  }

  @override
  void didUpdateWidget(LiveView old) {
    super.didUpdateWidget(old);
    if (old.size.fit != widget.size.fit) _setFit();
  }

  @override
  Widget build(BuildContext context) => ColoredBox(
        color: t.surface,
        child: Stack(children: [
          if (_error == null && _ready)
            Positioned.fill(
              child: InAppWebView(
                key: ValueKey('live-view-$_attempt'),
                initialUrlRequest: URLRequest(url: WebUri(widget.address)),
                initialUserScripts: UnmodifiableListView([_frameScript]),
                initialSettings: InAppWebViewSettings(
                  transparentBackground: false,
                  // Debuggable (chrome://inspect, Safari Web Inspector) in debug builds only.
                  isInspectable: kDebugMode,
                  // The page is the server's: no local files, no content:// providers, no
                  // cross-site cookies, and an https page never loads http subresources (the page
                  // itself may be http: a tailnet or emulator server).
                  allowFileAccess: false,
                  allowContentAccess: false,
                  allowFileAccessFromFileURLs: false,
                  allowUniversalAccessFromFileURLs: false,
                  thirdPartyCookiesEnabled: false,
                  mixedContentMode: MixedContentMode.MIXED_CONTENT_NEVER_ALLOW,
                  supportZoom: false,
                  disableContextMenu: true,
                  // The page takes pinch and pan itself (it does not scroll, `touch-action: none`): no
                  // overscroll glow. (disableVerticalScroll / disableHorizontalScroll would swallow
                  // the page's move events: not used.)
                  overScrollMode: OverScrollMode.NEVER,
                ),
                // Every touch that starts on the picture stays with it: neither the chat's scroll
                // nor anything around the webview may take a vertical (or sideways) pan.
                gestureRecognizers: {Factory<OneSequenceGestureRecognizer>(EagerGestureRecognizer.new)},
                onWebViewCreated: (controller) {
                  _controller = controller;
                  controller.addJavaScriptHandler(handlerName: _frameHandler, callback: _report);
                },
                onLoadStop: (_, _) {
                  if (mounted) setState(() => _loading = false);
                  _setFit();
                  _readFrame();
                },
                onReceivedError: (_, request, error) {
                  if (request.isForMainFrame ?? true) _failed(error.description);
                },
                onReceivedHttpError: (_, request, response) {
                  if (!(request.isForMainFrame ?? true)) return;
                  final status = response.statusCode ?? 0;
                  if (status == 401 || status == 404) {
                    _renew(status);
                  } else {
                    _failed('the server answered $status');
                  }
                },
              ),
            ),
          if (_loading && _error == null) const Positioned(left: 0, right: 0, top: 0, child: LinearProgressIndicator(minHeight: 2, color: t.accent, backgroundColor: Colors.transparent)),
          if (_error != null)
            Positioned.fill(
              child: Center(
                key: const Key('live-view-error'),
                child: Row(mainAxisSize: MainAxisSize.min, children: [
                  Flexible(child: Text("live view unavailable: $_error", maxLines: 2, overflow: TextOverflow.ellipsis, style: mono(size: 11.5, color: t.err))),
                  const SizedBox(width: 8),
                  TextButton(key: const Key('live-view-retry'), style: TextButton.styleFrom(minimumSize: const Size(64, 44)), onPressed: _retry, child: const Text('retry')),
                ]),
              ),
            ),
        ]),
      );
}
