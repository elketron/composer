import 'package:flutter/material.dart';

import '../theme/composer_theme.dart';
import '../theme/tokens.dart' as t;
import 'chat/navigation.dart';
import 'chat/session_tree.dart';
import 'server_list.dart';
import 'server_list_footer.dart';
import 'store_scope.dart';

/// The desktop sidebar: servers, then the active server's sessions.
class Rail extends StatelessWidget {
  const Rail({super.key});

  @override
  Widget build(BuildContext context) {
    final state = StoreScope.of(context).state;
    final server = state.activeServer!;
    return Container(
      width: 272,
      decoration: const BoxDecoration(color: t.surface, border: Border(right: BorderSide(color: t.border))),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        const ServerList(),
        const SizedBox(height: 8),
        const Divider(),
        Padding(
          padding: const EdgeInsets.fromLTRB(12, 8, 4, 4),
          child: Row(children: [
            Text('SESSIONS', style: mono(size: 11, color: t.faint, weight: FontWeight.w600)),
            const Spacer(),
            IconButton(
              key: const Key('new-session'),
              tooltip: 'New session',
              visualDensity: VisualDensity.compact,
              onPressed: () => newSession(context, server.url),
              icon: const Icon(Icons.add, size: 16, color: t.muted),
            ),
          ]),
        ),
        Expanded(
          child: SessionTree(
            url: server.url,
            status: state.statusOf(server.url),
            padding: const EdgeInsets.symmetric(horizontal: 6),
          ),
        ),
        const ServerListFooter(),
      ]),
    );
  }
}
