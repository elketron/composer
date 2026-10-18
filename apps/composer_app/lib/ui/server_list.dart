import 'dart:io';

import 'package:flutter/material.dart';

import '../state/actions.dart';
import '../state/types.dart';
import '../theme/composer_theme.dart';
import '../theme/tokens.dart' as t;
import 'add_server_page.dart';
import 'server_tile.dart';
import 'store_scope.dart';

/// The server switcher: the desktop rail, and the sheet on phones.
class ServerList extends StatelessWidget {
  const ServerList({super.key, this.onSelected});

  /// Called after switching (the sheet closes itself).
  final VoidCallback? onSelected;

  static final _desktop = Platform.isLinux || Platform.isWindows || Platform.isMacOS;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.of(context);
    final state = store.state;
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      Padding(
        padding: const EdgeInsets.fromLTRB(12, 14, 12, 8),
        child: Text('SERVERS', style: mono(size: 11, color: t.faint, weight: FontWeight.w600)),
      ),
      Expanded(
        child: ListView(padding: const EdgeInsets.symmetric(horizontal: 6), children: [
          for (final (i, server) in state.servers.indexed)
            Padding(
              padding: const EdgeInsets.only(bottom: 2),
              child: ServerTile(
                server: server,
                status: state.statusOf(server.url),
                selected: server.url == state.active,
                shortcut: _desktop && i < 9 ? 'ctrl ${i + 1}' : null,
                onTap: () {
                  store.dispatch(ServerSelected(server.url));
                  onSelected?.call();
                },
              ),
            ),
        ]),
      ),
      if (state.storage == StorageKind.file)
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
