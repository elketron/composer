// Opening pages and micro apps: in the pane beside the chat on wide screens, as a screen on phones.

import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../chat/navigation.dart';
import '../store_scope.dart';
import 'reader_screen.dart';

const readerRoute = 'reader';

void openPage(BuildContext context, String url, String page) {
  StoreScope.read(context).dispatch(PageRequested(url, page));
  _show(context, url);
}

void openApp(BuildContext context, String url, String site) {
  StoreScope.read(context).dispatch(AppRequested(url, site));
  _show(context, url);
}

void _show(BuildContext context, String url) {
  if (isWide(context) || ModalRoute.of(context)?.settings.name == readerRoute) return;
  Navigator.of(context).push(
    MaterialPageRoute<void>(settings: const RouteSettings(name: readerRoute), builder: (_) => ReaderScreen(url: url)),
  );
}
