// Opening chats: each as its own page.

import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../store_scope.dart';
import 'chat_page_route.dart';

void openSession(BuildContext context, String url, String session) {
  StoreScope.read(context).dispatch(SessionOpened(url, session));
  _push(context, url);
}

void newSession(BuildContext context, String url) {
  StoreScope.read(context).dispatch(NewSessionRequested(url));
  _push(context, url);
}

/// Show the server's chat as its own page; it fills in once the session is open.
void pushChat(BuildContext context, String url) => _push(context, url);

void _push(BuildContext context, String url) => _pushRoute(Navigator.of(context), StoreScope.read(context), url);

/// As [pushChat], for the app itself: it holds the navigator's key, so it has no context under it.
void pushChatPage(NavigatorState navigator, AppStore store, String url) => _pushRoute(navigator, store, url);

void _pushRoute(NavigatorState navigator, AppStore store, String url) {
  final route = ChatPageRoute(url: url);
  // Leaving the page closes the chat, unless the app unwound the page itself.
  navigator.push(route).then((_) {
    if (route.closeOnPop) store.dispatch(SessionClosed(url));
  });
}
