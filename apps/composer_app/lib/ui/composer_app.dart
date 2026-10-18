import 'package:flutter/material.dart';

import '../state/types.dart';
import '../theme/composer_theme.dart';
import 'shell.dart';
import 'store_scope.dart';

/// The app: theme, store, and notices shown as snackbars.
class ComposerApp extends StatefulWidget {
  const ComposerApp({super.key, required this.store});

  final AppStore store;

  @override
  State<ComposerApp> createState() => _ComposerAppState();
}

class _ComposerAppState extends State<ComposerApp> {
  final _messenger = GlobalKey<ScaffoldMessengerState>();
  Notice? _shown;

  @override
  void initState() {
    super.initState();
    widget.store.addListener(_onState);
  }

  @override
  void dispose() {
    widget.store.removeListener(_onState);
    super.dispose();
  }

  void _onState() {
    final notice = widget.store.state.notice;
    if (notice == null || identical(notice, _shown)) return;
    _shown = notice;
    _messenger.currentState?.showSnackBar(SnackBar(content: Text(notice.text)));
  }

  @override
  Widget build(BuildContext context) => StoreScope(
        store: widget.store,
        child: MaterialApp(
          title: 'composer',
          debugShowCheckedModeBanner: false,
          theme: composerTheme(),
          scaffoldMessengerKey: _messenger,
          home: const Shell(),
        ),
      );
}
