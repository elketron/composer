import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../theme/tokens.dart' as t;
import '../pages/page_navigation.dart';
import '../store_scope.dart';
import 'model_sheet.dart';

/// ⋮ for the open chat: pages, the model, compacting and stopping. The same on phones (app bar)
/// and wide screens (chat header).
class ChatMenu extends StatelessWidget {
  const ChatMenu({super.key, required this.url, required this.session});

  final String url;
  final SessionInfo? session;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.of(context);
    final status = store.state.statusOf(url);
    final session = this.session;
    return PopupMenuButton<String>(
      key: const Key('chat-menu'),
      tooltip: 'More',
      iconSize: 24,
      icon: const Icon(Icons.more_vert, size: 22, color: t.muted),
      enabled: session != null,
      onSelected: (choice) => switch (choice) {
        'pages' => openPage(context, url, status.page.page?.url ?? 'site://home'),
        'model' => showModelSheet(context, url),
        'compact' => store.dispatch(CompactRequested(url)),
        'stop' => store.dispatch(CancelRequested(url)),
        'delete' => _delete(context, session!),
        _ => null,
      },
      itemBuilder: (_) => [
        const PopupMenuItem(key: Key('pages'), value: 'pages', child: Text('Pages')),
        if (session != null)
          PopupMenuItem(value: 'model', child: Text(session.model == null ? 'Model…' : 'Model: ${session.model}…')),
        const PopupMenuItem(value: 'compact', child: Text('Compact (save memory, summarize)')),
        if (session != null && session.state != TurnState.idle) const PopupMenuItem(value: 'stop', child: Text('Stop the turn')),
        if (session != null) const PopupMenuItem(key: Key('delete-chat'), value: 'delete', child: Text('Delete chat…')),
      ],
    );
  }

  /// After a yes: the server deletes it, and its page closes.
  Future<void> _delete(BuildContext context, SessionInfo session) async {
    final store = StoreScope.read(context);
    final yes = await showDialog<bool>(
      context: context,
      builder: (dialog) => AlertDialog(
        title: const Text('Delete this chat?'),
        content: Text('#${session.id} and its transcript are deleted on the server.'),
        actions: [
          TextButton(onPressed: () => Navigator.pop(dialog, false), child: const Text('Cancel')),
          TextButton(
            key: const Key('confirm-delete'),
            onPressed: () => Navigator.pop(dialog, true),
            child: const Text('Delete', style: TextStyle(color: t.err)),
          ),
        ],
      ),
    );
    if (yes != true) return;
    store.dispatch(SessionDeleteRequested(url, session.id));
    if (context.mounted) Navigator.maybePop(context);
  }
}
