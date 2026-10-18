import 'package:flutter/material.dart';

import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../pages/page_navigation.dart';
import '../store_scope.dart';
import 'chat_view.dart';

/// A chat as its own page (phones).
class ChatPage extends StatelessWidget {
  const ChatPage({super.key, required this.url});

  final String url;

  @override
  Widget build(BuildContext context) {
    final status = StoreScope.of(context).state.statusOf(url);
    final chat = status.chat;
    final session = chat == null ? null : status.session(chat.session);
    return Scaffold(
      appBar: AppBar(
        titleSpacing: 0,
        title: Text.rich(TextSpan(children: [
          TextSpan(text: session?.agent ?? 'new session', style: mono(size: 15, color: t.text, weight: FontWeight.w600)),
          if (chat != null) TextSpan(text: '  #${chat.session}', style: mono(size: 12, color: t.faint)),
        ])),
        actions: [
          IconButton(
            tooltip: 'Sites',
            onPressed: () => openPage(context, url, 'site://home'),
            icon: const Icon(Icons.menu_book_outlined, size: 20, color: t.muted),
          ),
        ],
        bottom: const PreferredSize(preferredSize: Size.fromHeight(1), child: Divider()),
      ),
      body: chat == null
          ? const Center(child: SizedBox(width: 18, height: 18, child: CircularProgressIndicator(strokeWidth: 2)))
          : ChatView(url: url, status: status, chat: chat, header: false),
    );
  }
}
