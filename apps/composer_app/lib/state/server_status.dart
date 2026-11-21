import 'package:composer_protocol/composer_protocol.dart';

import 'chat_state.dart';
import 'page_state.dart';
import 'types.dart';
import 'sites.dart';

const _keep = Object();

/// What the app knows about a server right now (not persisted).
class ServerStatus {
  const ServerStatus({this.link = LinkState.connecting, this.error, this.retryAt, this.info, this.sessions = const [], this.approvals = const [], this.chat, this.page = const PageState(), this.sites = const [], this.unread = const {}, this.recent = const [], this.search = const PageSearch(), this.slotsUsed = 0, this.reader, this.models});

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

  /// From `/api/sites`, for the site list.
  final List<SiteSummary> sites;

  /// Sessions that finished a turn since you last looked at them.
  final Set<String> unread;

  /// Pages agents and apps changed lately, newest first.
  final List<RecentPage> recent;

  final PageSearch search;

  /// Model slots in use (`slots` events).
  final int slotsUsed;

  /// The provider's models and the server's own, as last asked (`model.list`); null: not yet.
  final CommandResultModels? models;

  /// What the reader screen shows; null: closed.
  final ReaderTab? reader;


  SiteSummary? site(String? name) => name == null ? null : sites.where((s) => s.name == name).firstOrNull;

  /// The sessions in a site now.
  List<SessionInfo> sessionsIn(String site) => sessions.where((s) => s.site == site).toList();

  /// The application the live view of [session] shows, or null: no live view for it.
  String? liveViewOf(SessionInfo? session) => liveViewApp(info: info, site: site(session?.site));

  SessionInfo? session(String id) => sessions.where((s) => s.id == id).firstOrNull;

  /// What a session is doing: `running`, `waiting`, `approval` or `idle`.
  String activity(SessionInfo s) => switch (s.state) {
        TurnState.running => 'running',
        TurnState.waiting => 'waiting',
        TurnState.awaitingApproval => 'approval',
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
    Set<String>? unread,
    List<RecentPage>? recent,
    PageSearch? search,
    int? slotsUsed,
    Object? reader = _keep,
    CommandResultModels? models,
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
        unread: unread ?? this.unread,
        recent: recent ?? this.recent,
        search: search ?? this.search,
        slotsUsed: slotsUsed ?? this.slotsUsed,
        reader: identical(reader, _keep) ? this.reader : reader as ReaderTab?,
        models: models ?? this.models,
      );
}
