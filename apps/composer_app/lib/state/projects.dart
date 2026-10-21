// Which project a session belongs to, as the server's project list counts them.

import 'package:composer_protocol/composer_protocol.dart';

import 'server_status.dart';

/// A session's project: its own, or for a worker (whose project is its ticket's worktree) its
/// orchestrator's.
String? sessionProject(ServerStatus status, SessionInfo session) =>
    session.ticket != null && session.parent != null ? status.session(session.parent!)?.project : session.project;

/// The sessions of a project (folder): its chats and orchestrators, and their workers.
List<SessionInfo> projectSessions(ServerStatus status, String folder) =>
    [for (final s in status.sessions) if (sessionProject(status, s) == folder) s];

/// A folder's last part (`/home/me/Projects/game` → `game`).
String projectName(String folder) => folder.split('/').where((p) => p.isNotEmpty).lastOrNull ?? folder;
