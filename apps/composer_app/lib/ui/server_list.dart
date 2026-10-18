import 'dart:io';

import 'package:flutter/material.dart';

import '../state/actions.dart';
import '../theme/composer_theme.dart';
import '../theme/tokens.dart' as t;
import 'server_tile.dart';
import 'store_scope.dart';

/// The servers, to switch between (the top of the desktop rail, and the sheet on phones).
class ServerList extends StatelessWidget {
  const ServerList({super.key, this.onSelected});

  /// Called after switching (the sheet closes itself).
  final VoidCallback? onSelected;

  static final _desktop = Platform.isLinux || Platform.isWindows || Platform.isMacOS;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.of(context);
    final state = store.state;
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
      Padding(
        padding: const EdgeInsets.fromLTRB(12, 14, 12, 8),
        child: Text('SERVERS', style: mono(size: 11, color: t.faint, weight: FontWeight.w600)),
      ),
      for (final (i, server) in state.servers.indexed)
        Padding(
          padding: const EdgeInsets.fromLTRB(6, 0, 6, 2),
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
    ]);
  }
}
