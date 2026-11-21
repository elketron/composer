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
        _ => null,
      },
      itemBuilder: (_) => [
        const PopupMenuItem(key: Key('pages'), value: 'pages', child: Text('Pages')),
        if (session != null)
          PopupMenuItem(value: 'model', child: Text(session.model == null ? 'Model…' : 'Model: ${session.model}…')),
        const PopupMenuItem(value: 'compact', child: Text('Compact (save memory, summarize)')),
        if (session != null && session.state != TurnState.idle) const PopupMenuItem(value: 'stop', child: Text('Stop the turn')),
      ],
    );
  }
}
