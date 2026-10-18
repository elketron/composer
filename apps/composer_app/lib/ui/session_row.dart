import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../theme/composer_theme.dart';
import '../theme/tokens.dart' as t;
import 'format.dart';

/// A session in a list: id, last message, state, age.
class SessionRow extends StatelessWidget {
  const SessionRow(this.session, {super.key});

  final SessionInfo session;

  @override
  Widget build(BuildContext context) {
    final busy = session.state != TurnState.idle;
    return Container(
      padding: const EdgeInsets.symmetric(vertical: 8),
      decoration: const BoxDecoration(border: Border(bottom: BorderSide(color: t.border))),
      child: Row(children: [
        SizedBox(width: 48, child: Text(session.id, style: mono(size: 12, color: t.accent))),
        Expanded(
          child: Text(
            session.preview ?? '(empty)',
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(fontSize: 13, color: session.preview == null ? t.faint : t.text),
          ),
        ),
        const SizedBox(width: 12),
        if (session.ticket != null) ...[Text('worker', style: mono(size: 11, color: t.info)), const SizedBox(width: 10)],
        if (busy) ...[
          Text(session.state.wire.replaceAll('_', ' '), style: mono(size: 11, color: t.warn)),
          const SizedBox(width: 10),
        ],
        Text(ago(session.updated), style: mono(size: 11, color: t.faint)),
      ]),
    );
  }
}
