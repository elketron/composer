import 'package:composer_protocol/composer_protocol.dart';

import 'server_status.dart';
import 'works.dart';

/// A server's sessions by what they need: you, running, and the rest (the Inbox tab).
class Inbox {
  const Inbox({required this.approvals, required this.gates, required this.unread, required this.busy, required this.rest});

  /// Recipes waiting for an answer in sessions that are listed.
  final List<ServerMessageApprovalRequest> approvals;

  /// Works waiting at a gate.
  final List<WorkSummary> gates;

  /// Finished something you haven't seen, and not running again.
  final List<SessionInfo> unread;
  final List<SessionInfo> busy;
  final List<SessionInfo> rest;

  /// What needs you: the number on the Inbox tab.
  int get needsYou => approvals.length + unread.length + gates.length;

  factory Inbox.of(ServerStatus status) {
    final sessions = status.sessions;
    final ids = {for (final s in sessions) s.id};
    // Workers show inside their orchestrator's card; on their own only when the orchestrator is gone.
    final top = sessions.where((s) => s.parent == null || !ids.contains(s.parent)).toList();
    bool running(SessionInfo s) =>
        isBusy(status.activity(s)) || sessions.any((w) => w.parent == s.id && isBusy(status.activity(w)));
    return Inbox(
      approvals: [for (final a in status.approvals) if (ids.contains(a.session)) a],
      gates: [for (final w in status.pipelines?.works ?? const <WorkSummary>[]) if (waitingStage(w) != null) w],
      unread: sessions.where((s) => status.unread.contains(s.id) && !running(s)).toList(),
      busy: top.where(running).toList(),
      rest: top.where((s) => !running(s) && !status.unread.contains(s.id)).toList(),
    );
  }
}

/// A session in one of these states is working (or waiting on a slot or you).
bool isBusy(String activity) => const {'running', 'waiting', 'starting', 'approval'}.contains(activity);
