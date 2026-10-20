import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/chat_items.dart';
import '../../state/chat_state.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;

/// While the agent works, what it is doing right now: the running step, thinking, or waiting.
class ActivityLine extends StatelessWidget {
  const ActivityLine({super.key, required this.chat, required this.state});

  final ChatState chat;
  final TurnState state;

  @override
  Widget build(BuildContext context) {
    final running = chat.items.whereType<ToolItem>().where((i) => i.result == null).lastOrNull;
    final streaming = chat.streaming;
    final text = switch (state) {
      TurnState.waiting => 'waiting for a model slot',
      TurnState.awaitingApproval => 'waiting for your approval',
      _ when running != null => '${running.name} ${running.summary}'.trim(),
      _ when streaming != null && streaming.content.isEmpty && streaming.thinking.isNotEmpty => 'thinking',
      _ when streaming != null && streaming.content.isNotEmpty => 'writing',
      _ => 'working',
    };
    return Row(children: [
      Container(
        width: 7,
        height: 7,
        decoration: BoxDecoration(
          color: t.warn,
          shape: BoxShape.circle,
          boxShadow: [BoxShadow(color: t.warn.withValues(alpha: 0.18), spreadRadius: 3)],
        ),
      ),
      const SizedBox(width: 10),
      Expanded(child: Text(text, maxLines: 1, overflow: TextOverflow.ellipsis, style: mono(size: 12, color: t.warn))),
    ]);
  }
}
