import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../state/actions.dart';
import '../theme/tokens.dart' as t;
import 'approval_bar.dart';
import 'chat/chat_view.dart';
import 'rail.dart';
import 'server_list.dart';
import 'server_list_footer.dart';
import 'server_page.dart';
import 'status_dot.dart';
import 'store_scope.dart';
import 'welcome_page.dart';

/// The app frame: a server rail beside the page on wide screens; on phones the title opens
/// the switcher. ctrl-1…9 switches servers.
class Shell extends StatelessWidget {
  const Shell({super.key});

  static const wide = 720.0;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.of(context);
    final state = store.state;
    if (!state.loaded) return const Scaffold();
    final server = state.activeServer;
    if (server == null) return const WelcomePage();
    final status = state.statusOf(server.url);
    final page = ServerPage(key: ValueKey(server.url), server: server, status: status);
    final chat = status.chat;

    return CallbackShortcuts(
      bindings: {
        for (var i = 0; i < state.servers.length && i < 9; i++)
          SingleActivator(LogicalKeyboardKey(LogicalKeyboardKey.digit1.keyId + i), control: true):
              () => store.dispatch(ServerSelected(state.servers[i].url)),
      },
      child: Focus(
        autofocus: true,
        child: LayoutBuilder(builder: (context, constraints) {
          if (constraints.maxWidth >= wide) {
            return Scaffold(
              body: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                const Rail(),
                Expanded(
                  child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                    const ApprovalBar(),
                    Expanded(
                      child: chat == null
                          ? page
                          : ChatView(
                              key: ValueKey('${server.url} ${chat.session}'),
                              url: server.url,
                              status: status,
                              chat: chat,
                              onClose: () => store.dispatch(SessionClosed(server.url)),
                            ),
                    ),
                  ]),
                ),
              ]),
            );
          }
          return Scaffold(
            appBar: AppBar(
              titleSpacing: 8,
              title: TextButton(
                key: const Key('switch-server'),
                style: TextButton.styleFrom(foregroundColor: t.text),
                onPressed: () => _openSwitcher(context),
                child: Row(mainAxisSize: MainAxisSize.min, children: [
                  StatusDot(status.link),
                  const SizedBox(width: 8),
                  Flexible(child: Text(server.name, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 16, fontWeight: FontWeight.w600))),
                  const SizedBox(width: 4),
                  Icon(Icons.expand_more, size: 18, color: t.muted),
                ]),
              ),
              bottom: const PreferredSize(preferredSize: Size.fromHeight(1), child: Divider()),
            ),
            body: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
              const ApprovalBar(),
              Expanded(child: page),
            ]),
          );
        }),
      ),
    );
  }

  void _openSwitcher(BuildContext context) => showModalBottomSheet<void>(
        context: context,
        showDragHandle: true,
        builder: (sheet) => SafeArea(
          child: SizedBox(
            height: MediaQuery.sizeOf(context).height * 0.5,
            child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
              Expanded(child: SingleChildScrollView(child: ServerList(onSelected: () => Navigator.pop(sheet)))),
              const ServerListFooter(),
            ]),
          ),
        ),
      );
}

