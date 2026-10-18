// Opening chats: in place on wide screens, as a page on phones.

import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../shell.dart';
import '../store_scope.dart';
import 'chat_page.dart';

bool isWide(BuildContext context) => MediaQuery.sizeOf(context).width >= Shell.wide;

void openSession(BuildContext context, String url, String session) {
  StoreScope.read(context).dispatch(SessionOpened(url, session));
  if (!isWide(context)) _push(context, url);
}

void newSession(BuildContext context, String url) {
  StoreScope.read(context).dispatch(NewSessionRequested(url));
  if (!isWide(context)) _push(context, url);
}

void _push(BuildContext context, String url) {
  final store = StoreScope.read(context);
  Navigator.of(context)
      .push(MaterialPageRoute<void>(builder: (_) => ChatPage(url: url)))
      .then((_) => store.dispatch(SessionClosed(url)));
}
