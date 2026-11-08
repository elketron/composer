// Opening pages and micro apps: the reader screen (one, whatever it shows).

import 'package:flutter/material.dart';

import '../../state/actions.dart';
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
  if (ModalRoute.of(context)?.settings.name == readerRoute) return;
  Navigator.of(context).push(
    MaterialPageRoute<void>(settings: const RouteSettings(name: readerRoute), builder: (_) => ReaderScreen(url: url)),
  );
}
