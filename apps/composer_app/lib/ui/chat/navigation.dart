// Opening chats: in place on wide screens, as a page on phones.

import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../shell.dart';
import '../store_scope.dart';
import 'chat_page_route.dart';

bool isWide(BuildContext context) => MediaQuery.sizeOf(context).width >= Shell.wide;

void openSession(BuildContext context, String url, String session) {
  StoreScope.read(context).dispatch(SessionOpened(url, session));
  if (!isWide(context)) _push(context, url);
}

void newSession(BuildContext context, String url) {
  StoreScope.read(context).dispatch(NewSessionRequested(url));
  if (!isWide(context)) _push(context, url);
}

/// Show the server's chat as its own page (phones); it fills in once the session is open.
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
