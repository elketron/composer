import 'package:flutter/material.dart';

import '../../state/chat_items.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import 'agent_text.dart';
import 'collapsible_row.dart';
import 'user_bubble.dart';

/// One chat item.
class ChatItemView extends StatelessWidget {
  const ChatItemView(this.item, {super.key});

  final ChatItem item;

  @override
  Widget build(BuildContext context) => switch (item) {
        UserItem() => UserBubble(item as UserItem),
        TextItem(:final text) => AgentText(text),
        ThinkingItem(:final text) => CollapsibleRow(label: TextSpan(text: 'thinking · ${_words(text)} words'), body: text),
        ToolItem(:final name, :final summary, :final result, :final isError) => CollapsibleRow(
            label: TextSpan(children: [
              TextSpan(text: name, style: const TextStyle(color: t.info)),
              TextSpan(text: summary.isEmpty ? '' : ' $summary'),
            ]),
            trailing: result == null
                ? const TextSpan(text: '…')
                : isError
                    ? const TextSpan(text: '✗', style: TextStyle(color: t.err))
                    : null,
            body: result,
          ),
        NoteItem(:final text) => Text(text, style: mono(size: 12, color: t.warn)),
        CompactedItem(:final summary, :final messages) =>
          CollapsibleRow(label: TextSpan(text: 'compacted $messages messages · summary'), body: summary),
      };

  static int _words(String text) => text.split(RegExp(r'\s+')).where((w) => w.isNotEmpty).length;
}
