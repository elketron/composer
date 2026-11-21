import 'package:flutter/material.dart';

import '../state/actions.dart';
import '../state/server_group.dart';
import '../theme/composer_theme.dart';
import '../theme/tokens.dart' as t;
import 'server_tile.dart';
import 'store_scope.dart';

/// The paired servers, grouped by host, to switch between (the sheet on phones).
class ServerList extends StatelessWidget {
  const ServerList({super.key, this.onSelected});

  /// Called after switching (the sheet closes itself).
  final VoidCallback? onSelected;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.of(context);
    final state = store.state;
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: [
      Padding(
        padding: const EdgeInsets.fromLTRB(12, 14, 12, 8),
        child: Text('SERVERS', style: mono(size: 11, color: t.faint, weight: FontWeight.w600)),
      ),
      for (final group in ServerGroup.group(state.servers)) ...[
        Padding(
          key: Key('host-${group.host}'),
          padding: const EdgeInsets.fromLTRB(12, 8, 12, 4),
          child: Text(group.host, style: mono(size: 11, color: t.faint)),
        ),
        for (final server in group.entries)
          Padding(
            padding: const EdgeInsets.fromLTRB(6, 0, 6, 2),
            child: ServerTile(
              key: Key('server-${server.url}'),
              server: server,
              status: state.statusOf(server.url),
              selected: server.url == state.active,
              onTap: () {
                store.dispatch(ServerSelected(server.url));
                onSelected?.call();
              },
            ),
          ),
      ],
    ]);
  }
}
