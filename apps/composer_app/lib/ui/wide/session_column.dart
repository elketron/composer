import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/server_entry.dart';
import '../../state/server_status.dart';
import '../../theme/tokens.dart' as t;
import '../home/chats_tab.dart';
import '../home/new_chat_sheet.dart';
import '../store_scope.dart';

/// The wide layout's session list: the phone's Chats (Needs you, Running, Recent) under the
/// server's name, with New and folding it away.
class SessionColumn extends StatelessWidget {
  const SessionColumn({super.key, required this.server, required this.status});

  static const width = 264.0;

  final ServerEntry server;
  final ServerStatus status;

  @override
  Widget build(BuildContext context) => Container(
        width: width,
        decoration: const BoxDecoration(border: Border(right: BorderSide(color: t.border))),
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Container(
            height: 48,
            padding: const EdgeInsets.only(left: 12, right: 4),
            decoration: const BoxDecoration(color: t.surface, border: Border(bottom: BorderSide(color: t.border))),
            child: Row(children: [
              Expanded(
                child: Text(server.name, overflow: TextOverflow.ellipsis, style: const TextStyle(fontWeight: FontWeight.w600, fontSize: 14)),
              ),
              FilledButton.icon(
                key: const Key('new-session'),
                style: FilledButton.styleFrom(minimumSize: const Size(0, 28), padding: const EdgeInsets.symmetric(horizontal: 10)),
                onPressed: () => showNewChat(context, server.url),
                icon: const Icon(Icons.add, size: 16),
                label: const Text('New', style: TextStyle(fontSize: 12.5)),
              ),
              IconButton(
                key: const Key('collapse-sessions'),
                tooltip: 'Fold the sessions (ctrl b)',
                visualDensity: VisualDensity.compact,
                onPressed: () => StoreScope.read(context).dispatch(const SessionsToggled()),
                icon: const Icon(Icons.chevron_left, size: 18, color: t.muted),
              ),
            ]),
          ),
          const SizedBox(height: 6),
          Expanded(child: ChatsTab(url: server.url, status: status, wide: true)),
        ]),
      );
}
