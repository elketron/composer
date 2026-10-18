import 'package:flutter/material.dart';

import '../state/actions.dart';
import '../state/server_entry.dart';
import '../state/server_status.dart';
import '../state/types.dart';
import '../theme/composer_theme.dart';
import '../theme/tokens.dart' as t;
import 'format.dart';
import 'info_badge.dart';
import 'pages/site_list.dart';
import 'chat/navigation.dart';
import 'chat/session_tree.dart';
import 'status_dot.dart';
import 'store_scope.dart';

/// One server: its state, what it runs, and its sessions (the chat comes in the next milestone).
class ServerPage extends StatelessWidget {
  const ServerPage({super.key, required this.server, required this.status});

  final ServerEntry server;
  final ServerStatus status;

  @override
  Widget build(BuildContext context) {
    final info = status.info;
    return ListView(padding: const EdgeInsets.fromLTRB(24, 20, 24, 24), children: [
      Row(children: [
        Expanded(child: Text(server.name, style: const TextStyle(fontSize: 20, fontWeight: FontWeight.w600))),
        PopupMenuButton<String>(
          tooltip: 'Server menu',
          icon: const Icon(Icons.more_horiz, color: t.muted),
          onSelected: (_) => _confirmRemove(context),
          itemBuilder: (_) => const [PopupMenuItem(value: 'remove', child: Text('Remove server'))],
        ),
      ]),
      Text(shortUrl(server.url), style: mono(size: 12)),
      const SizedBox(height: 14),
      Row(children: [
        StatusDot(status.link),
        const SizedBox(width: 8),
        Flexible(
          child: Text(
            status.link == LinkState.offline && status.error != null
                ? 'offline: ${status.error}'
                : linkLabel(status.link),
            key: const Key('link-state'),
            style: TextStyle(color: status.link == LinkState.offline ? t.err : t.muted, fontSize: 13),
          ),
        ),
        if (status.link == LinkState.offline) ...[
          const SizedBox(width: 12),
          TextButton(
            onPressed: () => StoreScope.read(context).dispatch(ReconnectRequested(server.url)),
            child: const Text('Reconnect'),
          ),
        ],
      ]),
      if (info != null) ...[
        const SizedBox(height: 14),
        Wrap(spacing: 8, runSpacing: 8, children: [
          InfoBadge('model', info.model),
          InfoBadge('slots', '${info.slots}'),
          InfoBadge('composer', info.version),
        ]),
      ],
      if (status.sites.isNotEmpty) ...[
        const SizedBox(height: 28),
        Text('SITES', style: mono(size: 11, color: t.faint, weight: FontWeight.w600)),
        const SizedBox(height: 10),
        SiteList(url: server.url, status: status),
      ],
      const SizedBox(height: 28),
      Row(children: [
        Text('SESSIONS', style: mono(size: 11, color: t.faint, weight: FontWeight.w600)),
        const Spacer(),
        TextButton.icon(
          key: const Key('new-session-page'),
          onPressed: status.link == LinkState.online ? () => newSession(context, server.url) : null,
          icon: const Icon(Icons.add, size: 16),
          label: const Text('New session'),
        ),
      ]),
      const SizedBox(height: 4),
      SessionTree(url: server.url, status: status, shrinkWrap: true),
    ]);
  }

  Future<void> _confirmRemove(BuildContext context) async {
    final store = StoreScope.read(context);
    final remove = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        backgroundColor: t.surface,
        title: Text('Remove ${server.name}?'),
        content: const Text('This device forgets the server and its token. To also revoke the token, run composer devices revoke on the server.'),
        actions: [
          TextButton(onPressed: () => Navigator.pop(context, false), child: const Text('Cancel')),
          FilledButton(
            style: FilledButton.styleFrom(backgroundColor: t.err),
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Remove'),
          ),
        ],
      ),
    );
    if (remove == true) store.dispatch(ServerRemoved(server.url));
  }
}
