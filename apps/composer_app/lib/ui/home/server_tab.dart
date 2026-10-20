import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/server_entry.dart';
import '../../state/server_status.dart';
import '../../state/types.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../add_server_page.dart';
import '../format.dart';
import '../status_dot.dart';
import '../store_scope.dart';
import 'section_label.dart';

/// Connections, out of the way of daily use: this server, the others, adding and removing.
class ServerTab extends StatelessWidget {
  const ServerTab({super.key, required this.server, required this.status});

  final ServerEntry server;
  final ServerStatus status;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.of(context);
    final state = store.state;
    final info = status.info;
    final offline = status.link == LinkState.offline;
    Widget kv(String key, String value) => Padding(
          padding: const EdgeInsets.only(top: 6),
          child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
            SizedBox(width: 72, child: Text(key, style: mono(size: 11.5, color: t.faint))),
            Expanded(child: Text(value, style: mono(size: 12.5, color: t.text))),
          ]),
        );
    return ListView(padding: const EdgeInsets.fromLTRB(14, 0, 14, 24), children: [
      const SectionLabel('This server'),
      Container(
        padding: const EdgeInsets.all(12),
        decoration: BoxDecoration(color: t.surface, border: Border.all(color: t.border), borderRadius: BorderRadius.circular(t.radius)),
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Row(children: [
            StatusDot(status.link),
            const SizedBox(width: 8),
            Text(server.name, style: const TextStyle(fontWeight: FontWeight.w600, fontSize: 15)),
            const SizedBox(width: 8),
            Expanded(
              child: Text(
                offline && status.error != null ? status.error! : linkLabel(status.link),
                maxLines: 2,
                style: TextStyle(fontSize: 12, color: offline ? t.err : t.muted),
              ),
            ),
            if (offline)
              TextButton(onPressed: () => store.dispatch(ReconnectRequested(server.url)), child: const Text('Retry')),
          ]),
          const SizedBox(height: 4),
          kv('address', shortUrl(server.url)),
          if (info != null) ...[
            kv('model', info.model),
            kv('slots', '${status.slotsUsed} of ${info.slots} in use'),
            kv('version', 'composer ${info.version}'),
            if ((info.projectsDir ?? '').isNotEmpty) kv('projects', info.projectsDir!),
          ],
        ]),
      ),
      const SectionLabel('Servers'),
      Container(
        decoration: BoxDecoration(color: t.surface, border: Border.all(color: t.border), borderRadius: BorderRadius.circular(t.radius)),
        child: Column(children: [
          for (final (i, other) in state.servers.indexed) ...[
            if (i > 0) const Divider(height: 1),
            InkWell(
              onTap: () => store.dispatch(ServerSelected(other.url)),
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
                child: Row(children: [
                  StatusDot(state.statusOf(other.url).link),
                  const SizedBox(width: 10),
                  Expanded(
                    child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                      Text(other.name, style: const TextStyle(fontSize: 14)),
                      Text(
                        state.statusOf(other.url).link == LinkState.offline
                            ? '${shortUrl(other.url)} · offline'
                            : shortUrl(other.url),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: mono(size: 11, color: t.faint),
                      ),
                    ]),
                  ),
                  if (other.url == server.url) Text('this one', style: mono(size: 11, color: t.faint)),
                ]),
              ),
            ),
          ],
        ]),
      ),
      const SizedBox(height: 10),
      OutlinedButton.icon(
        key: const Key('add-server'),
        style: OutlinedButton.styleFrom(minimumSize: const Size.fromHeight(42)),
        onPressed: () => AddServerPage.open(context),
        icon: const Icon(Icons.add, size: 18),
        label: const Text('Add a server'),
      ),
      const SizedBox(height: 4),
      TextButton(
        style: TextButton.styleFrom(foregroundColor: t.err),
        onPressed: () => _confirmRemove(context),
        child: Text('Remove ${server.name} from this device'),
      ),
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
          FilledButton(style: FilledButton.styleFrom(backgroundColor: t.err), onPressed: () => Navigator.pop(context, true), child: const Text('Remove')),
        ],
      ),
    );
    if (remove == true) store.dispatch(ServerRemoved(server.url));
  }
}
