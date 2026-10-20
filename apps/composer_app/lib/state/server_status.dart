import 'package:composer_protocol/composer_protocol.dart';

import 'chat_state.dart';
import 'page_state.dart';
import 'types.dart';

const _keep = Object();

/// What the app knows about a server right now (not persisted).
class ServerStatus {
  const ServerStatus({this.link = LinkState.connecting, this.error, this.retryAt, this.info, this.sessions = const [], this.approvals = const [], this.chat, this.page = const PageState(), this.sites = const [], this.workers = const {}, this.agents = const [], this.projects = const [], this.unread = const {}, this.recent = const [], this.search = const PageSearch(), this.slotsUsed = 0});

  static const initial = ServerStatus();

  final LinkState link;

  /// Why the last connection attempt failed.
  final String? error;

  /// When the next attempt is due, while offline.
  final DateTime? retryAt;

  final ServerInfo? info;

  /// Newest first.
  final List<SessionInfo> sessions;

  /// Recipes waiting for an answer, from any of the server's sessions.
  final List<ServerMessageApprovalRequest> approvals;

  /// The open session, if any.
  final ChatState? chat;

  final PageState page;

  /// From `/api/sites`, for the site list and the "App" button.
  final List<SiteSummary> sites;

  /// Workers' last `worker.status`, by session id (seen since the app connected).
  final Map<String, WorkerState> workers;

  /// From `/api/agents`, for the New chat sheet.
  final List<AgentSummary> agents;

  /// From `/api/projects`, most recently changed first.
  final List<ProjectSummary> projects;

  /// Sessions that finished something (a turn, a worker) since you last looked at them.
  final Set<String> unread;

  /// Pages agents and apps changed lately, newest first.
  final List<RecentPage> recent;

  final PageSearch search;

  /// Model slots in use (`slots` events).
  final int slotsUsed;

  bool hasApp(String site) => sites.any((s) => s.name == site && s.appBuilt);

  SessionInfo? session(String id) => sessions.where((s) => s.id == id).firstOrNull;

  /// The orchestration `id` belongs to: its orchestrator first, then the workers it spawned.
  /// Empty for a session without workers or orchestrator.
  List<SessionInfo> orchestration(String id) {
    final rootId = session(id)?.parent ?? id;
    final root = session(rootId);
    if (root == null) return const [];
    final members = [root, ...sessions.where((s) => s.parent == rootId)];
    return members.length > 1 ? members : const [];
  }

  /// What a session is doing: `running`, `waiting`, `approval`, `idle`, or a worker's
  /// `starting` / `done` / `failed`.
  String activity(SessionInfo s) => switch ((s.state, workers[s.id] ?? s.worker)) {
        // A finished worker is finished, whatever its last turn state said.
        (_, WorkerState.done) => 'done',
        (_, WorkerState.failed) => 'failed',
        (TurnState.running, _) => 'running',
        (TurnState.waiting, _) => 'waiting',
        (TurnState.awaitingApproval, _) => 'approval',
        (TurnState.idle, WorkerState.starting) => 'starting',
        _ => 'idle',
      };

  ServerStatus copyWith({
    LinkState? link,
    Object? error = _keep,
    Object? retryAt = _keep,
    Object? info = _keep,
    List<SessionInfo>? sessions,
    List<ServerMessageApprovalRequest>? approvals,
    Object? chat = _keep,
    PageState? page,
    List<SiteSummary>? sites,
    Map<String, WorkerState>? workers,
    List<AgentSummary>? agents,
    List<ProjectSummary>? projects,
    Set<String>? unread,
    List<RecentPage>? recent,
    PageSearch? search,
    int? slotsUsed,
  }) =>
      ServerStatus(
        link: link ?? this.link,
        error: identical(error, _keep) ? this.error : error as String?,
        retryAt: identical(retryAt, _keep) ? this.retryAt : retryAt as DateTime?,
        info: identical(info, _keep) ? this.info : info as ServerInfo?,
        sessions: sessions ?? this.sessions,
        approvals: approvals ?? this.approvals,
        chat: identical(chat, _keep) ? this.chat : chat as ChatState?,
        page: page ?? this.page,
        sites: sites ?? this.sites,
        workers: workers ?? this.workers,
        agents: agents ?? this.agents,
        projects: projects ?? this.projects,
        unread: unread ?? this.unread,
        recent: recent ?? this.recent,
        search: search ?? this.search,
        slotsUsed: slotsUsed ?? this.slotsUsed,
      );
}
