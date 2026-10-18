import 'package:flutter/material.dart';

import '../theme/composer_theme.dart';
import 'shell.dart';
import 'store_scope.dart';

class ComposerApp extends StatelessWidget {
  const ComposerApp({super.key, required this.store});

  final AppStore store;

  @override
  Widget build(BuildContext context) => StoreScope(
        store: store,
        child: MaterialApp(
          title: 'composer',
          debugShowCheckedModeBanner: false,
          theme: composerTheme(),
          home: const Shell(),
        ),
      );
}
