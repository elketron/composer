import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../format.dart';

/// A session in the tree: agent, id, state or age, and the last message.
class SessionTile extends StatelessWidget {
  const SessionTile({super.key, required this.session, required this.selected, required this.onTap, this.depth = 0});

  final SessionInfo session;
  final bool selected;
  final VoidCallback onTap;

  /// Workers sit one level in.
  final int depth;

  @override
  Widget build(BuildContext context) {
    final busy = session.state != TurnState.idle;
    final waitsOnYou = session.state == TurnState.awaitingApproval;
    return Padding(
      padding: EdgeInsets.only(left: depth * 16.0, bottom: 2),
      child: Material(
        color: selected ? t.raised : Colors.transparent,
        borderRadius: BorderRadius.circular(t.radius),
        child: InkWell(
          key: Key('session-${session.id}'),
          borderRadius: BorderRadius.circular(t.radius),
          onTap: onTap,
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 7),
            child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
              Row(children: [
                Text(session.agent, style: mono(size: 12, color: depth > 0 ? t.info : t.text)),
                Text('  #${session.id}', style: mono(size: 11, color: t.faint)),
                const Spacer(),
                Text(
                  busy ? (waitsOnYou ? 'needs approval' : session.state.wire) : ago(session.updated),
                  style: mono(size: 10, color: busy ? t.warn : t.faint),
                ),
              ]),
              if (session.preview != null) ...[
                const SizedBox(height: 2),
                Text(
                  session.preview!,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(fontSize: 13, color: t.muted),
                ),
              ],
            ]),
          ),
        ),
      ),
    );
  }
}
