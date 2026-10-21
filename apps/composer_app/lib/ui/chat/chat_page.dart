import 'package:flutter/material.dart';

import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../home/activity_dot.dart';
import '../pages/page_navigation.dart';
import '../store_scope.dart';
import 'chat_menu.dart';
import 'chat_view.dart';

/// A chat as its own page (phones): agent, state and project on top; ⋮ for the project,
/// compacting and stopping.
class ChatPage extends StatelessWidget {
  const ChatPage({super.key, required this.url});

  final String url;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.of(context);
    final status = store.state.statusOf(url);
    final chat = status.chat;
    final session = chat == null ? null : status.session(chat.session);
    final activity = session == null ? 'idle' : status.activity(session);
    final project = session?.project?.split('/').last;
    return Scaffold(
      appBar: AppBar(
        titleSpacing: 0,
        title: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text(session?.agent ?? 'new chat', style: mono(size: 15, color: t.text, weight: FontWeight.w600)),
          if (chat != null)
            Row(children: [
              ActivityDot(activity, size: 6),
              const SizedBox(width: 6),
              Flexible(
                child: Text(
                  [activity == 'approval' ? 'needs approval' : activity, project ?? 'no project', '#${chat.session}'].join(' · '),
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(fontSize: 11.5, color: t.muted),
                ),
              ),
            ]),
        ]),
        actions: [
          IconButton(
            tooltip: 'Pages',
            onPressed: () => openPage(context, url, status.page.page?.url ?? 'site://home'),
            icon: const Icon(Icons.description_outlined, size: 20, color: t.muted),
          ),
          ChatMenu(url: url, session: session),
        ],
        bottom: const PreferredSize(preferredSize: Size.fromHeight(1), child: Divider()),
      ),
      body: chat == null
          ? const Center(child: SizedBox(width: 18, height: 18, child: CircularProgressIndicator(strokeWidth: 2)))
          : ChatView(url: url, status: status, chat: chat, header: false),
    );
  }
}
