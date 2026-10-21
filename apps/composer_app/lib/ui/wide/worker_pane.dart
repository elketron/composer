import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/chat_state.dart';
import '../../state/server_status.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../chat/chat_view.dart';
import '../home/activity_dot.dart';
import '../store_scope.dart';

/// Another session live beside the open chat (a worker beside its orchestrator): its steps,
/// streaming text and approvals. "Open as chat" makes it the open chat.
class WorkerPane extends StatelessWidget {
  const WorkerPane({super.key, required this.url, required this.status, required this.chat});

  final String url;
  final ServerStatus status;
  final ChatState chat;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.read(context);
    final session = status.session(chat.session);
    final activity = session == null ? 'idle' : status.activity(session);
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      Container(
        height: 36,
        padding: const EdgeInsets.only(left: 12, right: 4),
        decoration: const BoxDecoration(border: Border(bottom: BorderSide(color: t.border))),
        child: Row(children: [
          ActivityDot(activity, size: 6),
          const SizedBox(width: 8),
          Expanded(
            child: Text.rich(
              TextSpan(children: [
                TextSpan(text: '${session?.agent ?? 'session'} #${chat.session}', style: mono(size: 12, color: t.text)),
                if (session?.ticket != null) TextSpan(text: ' · ticket ${session!.ticket}', style: mono(size: 12, color: t.info)),
                TextSpan(text: ' · ${activity == 'approval' ? 'needs approval' : activity}', style: mono(size: 12, color: t.muted)),
              ]),
              overflow: TextOverflow.ellipsis,
            ),
          ),
          TextButton(
            key: const Key('open-as-chat'),
            style: TextButton.styleFrom(foregroundColor: t.muted, visualDensity: VisualDensity.compact),
            onPressed: () {
              store.dispatch(SessionOpened(url, chat.session));
              store.dispatch(SideClosed(url));
            },
            child: const Text('Open as chat'),
          ),
        ]),
      ),
      Expanded(child: ChatView(url: url, status: status, chat: chat, header: false, compact: true)),
    ]);
  }
}
