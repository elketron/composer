import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/chat_rows.dart';
import '../../state/run_kind.dart';
import '../../state/server_status.dart';
import '../../state/sites.dart';
import '../../theme/tokens.dart' as t;
import '../home/activity_dot.dart';

/// The session's run kind, and its state line with the session id.
class ChatTitle extends StatelessWidget {
  const ChatTitle({super.key, required this.status, required this.session});

  final ServerStatus status;
  final SessionInfo? session;

  @override
  Widget build(BuildContext context) {
    final chat = status.chat;
    final activity = session == null ? 'idle' : status.activity(session!);
    return Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
      Text(
        session == null ? 'new chat' : sessionLabel(session!),
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        style: const TextStyle(fontSize: 15, fontWeight: FontWeight.w600),
      ),
      if (chat != null)
        Row(children: [
          ActivityDot(activity),
          const SizedBox(width: 6),
          Flexible(
            child: Text(
              '${stateLine(activity, stepsSinceLastMessage(chat.items))} · #${chat.session}',
              key: const Key('session-state'),
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: const TextStyle(fontSize: 11, color: t.muted),
            ),
          ),
        ]),
    ]);
  }
}
