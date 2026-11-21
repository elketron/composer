import 'package:flutter/material.dart';

import '../../state/server_mark.dart';
import '../store_scope.dart';
import '../server_badge.dart';
import 'reader.dart';

/// The reader as its own screen (phones).
class ReaderScreen extends StatelessWidget {
  const ReaderScreen({super.key, required this.url});

  final String url;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.of(context);
    final status = store.state.statusOf(url);
    final mark = ServerMark.of(url, info: status.info, entry: store.state.serverFor(url));
    return Scaffold(
      body: SafeArea(
        child: Column(children: [
          ServerStrip(mark: mark),
          Expanded(child: Reader(url: url, status: status, onClose: () => Navigator.of(context).maybePop())),
        ]),
      ),
    );
  }
}
