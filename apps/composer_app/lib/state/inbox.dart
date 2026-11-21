import 'package:composer_protocol/composer_protocol.dart';

import 'server_status.dart';

/// A server's sessions by what they need: you, running, and the rest (the Inbox tab).
class Inbox {
  const Inbox({required this.approvals, required this.unread, required this.busy, required this.rest});

  /// Recipes waiting for an answer in sessions that are listed.
  final List<ServerMessageApprovalRequest> approvals;

  /// Finished something you haven't seen, and not running again.
  final List<SessionInfo> unread;
  final List<SessionInfo> busy;
  final List<SessionInfo> rest;

  /// What needs you: the number on the Inbox tab.
  int get needsYou => approvals.length + unread.length;

  factory Inbox.of(ServerStatus status) {
    final sessions = status.sessions;
    final ids = {for (final s in sessions) s.id};
    bool running(SessionInfo s) => isBusy(status.activity(s));
    return Inbox(
      approvals: [for (final a in status.approvals) if (ids.contains(a.session)) a],
      unread: sessions.where((s) => status.unread.contains(s.id) && !running(s)).toList(),
      busy: sessions.where(running).toList(),
      rest: sessions.where((s) => !running(s) && !status.unread.contains(s.id)).toList(),
    );
  }
}

/// A session in one of these states is working (or waiting on a slot or you).
bool isBusy(String activity) => const {'running', 'waiting', 'approval'}.contains(activity);
