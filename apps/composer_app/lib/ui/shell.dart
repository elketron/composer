import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../state/actions.dart';
import '../theme/tokens.dart' as t;
import 'approval_bar.dart';
import 'chat/chat_view.dart';
import 'pages/reader.dart';
import 'rail.dart';
import 'home/phone_home.dart';
import 'server_page.dart';
import 'store_scope.dart';
import 'welcome_page.dart';

/// The app frame: a server rail beside the page on wide screens; on phones `PhoneHome` (tabs,
/// the title opens the switcher). ctrl-1…9 switches servers.
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
            final main = chat == null
                ? page
                : ChatView(
                    key: ValueKey('${server.url} ${chat.session}'),
                    url: server.url,
                    status: status,
                    chat: chat,
                    onClose: () => store.dispatch(SessionClosed(server.url)),
                  );
            return Scaffold(
              body: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                const Rail(),
                Expanded(
                  child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                    const ApprovalBar(),
                    Expanded(
                      child: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                        Expanded(child: main),
                        // The reader beside the chat, as the TUI's second pane.
                        if (status.page.isOpen)
                          Container(
                            width: (constraints.maxWidth - 272) * 0.5,
                            decoration: const BoxDecoration(border: Border(left: BorderSide(color: t.border))),
                            child: Reader(
                              url: server.url,
                              status: status,
                              onClose: () => store.dispatch(ReaderClosed(server.url)),
                            ),
                          ),
                      ]),
                    ),
                  ]),
                ),
              ]),
            );
          }
          return PhoneHome(key: ValueKey(server.url), server: server, status: status);
        }),
      ),
    );
  }
}
