import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/server_status.dart';
import '../../theme/tokens.dart' as t;
import 'navigation.dart';
import 'session_tile.dart';

/// A server's sessions, newest first, with workers under the session that spawned them.
class SessionTree extends StatelessWidget {
  const SessionTree({super.key, required this.url, required this.status, this.shrinkWrap = false, this.padding});

  final String url;
  final ServerStatus status;
  final bool shrinkWrap;
  final EdgeInsets? padding;

  /// Depth-first: each session followed by its workers.
  static List<(SessionInfo, int)> flatten(List<SessionInfo> sessions) {
    final ids = sessions.map((s) => s.id).toSet();
    final children = <String, List<SessionInfo>>{};
    for (final s in sessions) {
      if (s.parent != null && ids.contains(s.parent)) children.putIfAbsent(s.parent!, () => []).add(s);
    }
    final out = <(SessionInfo, int)>[];
    void visit(SessionInfo s, int depth) {
      out.add((s, depth));
      for (final child in children[s.id] ?? const <SessionInfo>[]) {
        visit(child, depth + 1);
      }
    }

    for (final s in sessions.where((s) => s.parent == null || !ids.contains(s.parent))) {
      visit(s, 0);
    }
    return out;
  }

  @override
  Widget build(BuildContext context) {
    final rows = flatten(status.sessions);
    if (rows.isEmpty) {
      return Padding(
        padding: padding ?? EdgeInsets.zero,
        child: const Text('No sessions yet.', style: TextStyle(color: t.faint, fontSize: 13)),
      );
    }
    return ListView(
      shrinkWrap: shrinkWrap,
      physics: shrinkWrap ? const NeverScrollableScrollPhysics() : null,
      padding: padding,
      children: [
        for (final (session, depth) in rows)
          SessionTile(
            session: session,
            depth: depth,
            selected: status.chat?.session == session.id,
            onTap: () => openSession(context, url, session.id),
          ),
      ],
    );
  }
}
