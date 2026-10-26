import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../store_scope.dart';

/// Above the oldest message shown. Built when you scroll near it (or the chat is short), it
/// fetches the ones before.
class OlderMessages extends StatelessWidget {
  const OlderMessages({super.key, required this.url, required this.session, required this.loading});

  final String url;
  final String session;
  final bool loading;

  @override
  Widget build(BuildContext context) {
    if (!loading) {
      final store = StoreScope.read(context);
      WidgetsBinding.instance.addPostFrameCallback((_) => store.dispatch(OlderRequested(url, session)));
    }
    return const Center(
      child: Padding(
        padding: EdgeInsets.symmetric(vertical: 8),
        child: SizedBox(width: 14, height: 14, child: CircularProgressIndicator(strokeWidth: 2)),
      ),
    );
  }
}
