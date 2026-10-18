import 'package:flutter/material.dart';

import '../state/types.dart';
import '../theme/composer_theme.dart';
import '../theme/tokens.dart' as t;
import 'add_server_page.dart';
import 'store_scope.dart';

/// "Add server", and a note when tokens are kept in a file.
class ServerListFooter extends StatelessWidget {
  const ServerListFooter({super.key});

  @override
  Widget build(BuildContext context) {
    final storage = StoreScope.of(context).state.storage;
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      if (storage == StorageKind.file)
        Padding(
          padding: const EdgeInsets.fromLTRB(12, 0, 12, 8),
          child: Tooltip(
            message: 'No keyring (Secret Service) was found, so device tokens are kept in a file only you can read, as `composer login` does.',
            child: Text('tokens kept in a file', style: mono(size: 10, color: t.faint)),
          ),
        ),
      Padding(
        padding: const EdgeInsets.fromLTRB(8, 0, 8, 10),
        child: OutlinedButton.icon(
          onPressed: () => AddServerPage.open(context),
          icon: const Icon(Icons.add, size: 16),
          label: const Text('Add server'),
        ),
      ),
    ]);
  }
}
