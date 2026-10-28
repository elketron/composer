import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import 'dart:async';

import '../servers/notifier.dart';
import '../state/actions.dart';
import '../state/types.dart';
import '../theme/composer_theme.dart';
import 'chat/chat_page_route.dart';
import 'chat/navigation.dart';
import 'pages/page_navigation.dart';
import 'shell.dart';
import 'store_scope.dart';

/// The app: theme, store, notices shown as snackbars, and taps on system notifications.
class ComposerApp extends StatefulWidget {
  const ComposerApp({super.key, required this.store, required this.notifier});

  final AppStore store;
  final Notifier notifier;

  @override
  State<ComposerApp> createState() => _ComposerAppState();
}

class _ComposerAppState extends State<ComposerApp> {
  final _messenger = GlobalKey<ScaffoldMessengerState>();
  final _navigator = GlobalKey<NavigatorState>();
  StreamSubscription<NotificationTap>? _taps;
  Notice? _shown;

  @override
  void initState() {
    super.initState();
    widget.store.addListener(_onState);
    _taps = widget.notifier.taps.listen(_onTap);
  }

  @override
  void dispose() {
    widget.store.removeListener(_onState);
    _taps?.cancel();
    super.dispose();
  }

  void _onState() {
    final notice = widget.store.state.notice;
    if (notice == null || identical(notice, _shown)) return;
    _shown = notice;
    _messenger.currentState?.showSnackBar(SnackBar(content: Text(notice.text)));
  }

  /// Approve / Deny answer right away; the notification itself opens its session.
  void _onTap(NotificationTap tap) {
    final store = widget.store;
    final approval = tap.approval;
    if (approval != null && (tap.action == 'approve' || tap.action == 'deny')) {
      store.dispatch(ApprovalAnswered(tap.url, approval, tap.action == 'approve' ? ApprovalDecision.approve : ApprovalDecision.deny));
      return;
    }
    store.dispatch(ServerSelected(tap.url));
    final session = tap.session;
    final page = tap.page;
    if (session == null) {
      // A stage at its gate: the pipeline's board. A reminder: its page.
      final context = _navigator.currentContext;
      if (context == null) return;
      if (tap.app != null) {
        openApp(context, tap.url, tap.app!);
      } else if (page != null) {
        openPage(context, tap.url, page);
      }
      return;
    }
    store.dispatch(SessionOpened(tap.url, session));
    final context = _navigator.currentContext;
    if (context != null && !isWide(context)) {
      // Unwinding the page it stands on is not the user closing that chat: the store already
      // let it go with SessionOpened, and the page being pushed owns the chat from now on.
      final navigator = _navigator.currentState!;
      navigator.popUntil((route) {
        if (route is ChatPageRoute) route.closeOnPop = false;
        return route.isFirst;
      });
      pushChatPage(navigator, store, tap.url);
    }
  }

  @override
  Widget build(BuildContext context) => StoreScope(
        store: widget.store,
        child: MaterialApp(
          title: 'composer',
          debugShowCheckedModeBanner: false,
          theme: composerTheme(),
          scaffoldMessengerKey: _messenger,
          navigatorKey: _navigator,
          home: const Shell(),
        ),
      );
}
