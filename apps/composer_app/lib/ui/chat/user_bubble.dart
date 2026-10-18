import 'package:flutter/material.dart';

import '../../state/chat_items.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;

/// Your message (or an app's, or a worker's report), on the right with the tinted background.
class UserBubble extends StatelessWidget {
  const UserBubble(this.item, {super.key});

  final UserItem item;

  @override
  Widget build(BuildContext context) => Align(
        alignment: Alignment.centerRight,
        child: FractionallySizedBox(
          widthFactor: 0.85,
          alignment: Alignment.centerRight,
          child: Align(
            alignment: Alignment.centerRight,
            child: Container(
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
              decoration: BoxDecoration(
                color: t.you,
                border: Border.all(color: t.accent.withValues(alpha: 0.15)),
                borderRadius: const BorderRadius.only(
                  topLeft: Radius.circular(t.radius),
                  topRight: Radius.circular(t.radius),
                  bottomLeft: Radius.circular(t.radius),
                  bottomRight: Radius.circular(t.radiusSm),
                ),
              ),
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, mainAxisSize: MainAxisSize.min, children: [
                if (item.source != null) ...[
                  Text(item.source!, style: mono(size: 11, color: t.faint)),
                  const SizedBox(height: 2),
                ],
                SelectableText(item.text, style: const TextStyle(fontSize: 15, height: 1.4)),
              ]),
            ),
          ),
        ),
      );
}
