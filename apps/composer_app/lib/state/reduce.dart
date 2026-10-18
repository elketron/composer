import 'package:composer_protocol/composer_protocol.dart';

import 'actions.dart';
import 'app_state.dart';
import 'chat_events.dart';
import 'chat_state.dart';
import 'effects.dart';
import 'page_state.dart';
import 'site_url.dart';
import 'server_entry.dart';
import 'server_status.dart';
import 'types.dart';

/// The app's reducer: pure, so it is tested without IO.
(AppState, List<AppEffect>) reduce(AppState s, AppAction action) => switch (action) {
      AppStarted() => (s, const [LoadServers()]),
      AppResumed() => (
          s,
          [for (final server in s.servers) if (s.statusOf(server.url).link == LinkState.offline) Reconnect(server.url)],
        ),
      ServersLoaded(:final servers, :final storage) => (
          s.copyWith(
            loaded: true,
            storage: storage,
            servers: servers,
            status: {for (final server in servers) server.url: ServerStatus.initial},
            active: servers.firstOrNull?.url,
          ),
          [for (final server in servers) Connect(server)],
        ),
      ServerSelected(:final url) => s.servers.any((x) => x.url == url) ? (s.copyWith(active: url), const []) : (s, const []),
      PairingReset() => (s.copyWith(pairing: const PairingState()), const []),
      PairRequested(:final url, :final code, :final deviceName) => (
          s.copyWith(pairing: const PairingState(busy: true)),
          [Pair(url: url, code: code, deviceName: deviceName)],
        ),
      PairFailed(:final message) => (s.copyWith(pairing: PairingState(error: message)), const []),
      PairSucceeded(:final server, :final info) => _paired(s, server.withName(info.name), info),
      ServerRemoved(:final url) => _removed(s, url),
      ReconnectRequested(:final url) => (s, [Reconnect(url)]),
      LinkChanged(:final url, :final link, :final error, :final retryAt) => _link(s, url, link, error, retryAt),
      InfoReceived(:final url, :final info) => _info(s, url, info),
      ServerEvent(:final url, :final message) => _serverEvent(s, url, message),
      SessionOpened(:final url, :final session) => _open(s, url, session),
      SessionClosed(:final url) => _close(s, url),
      NewSessionRequested(:final url, :final agent) => (s, [CreateSession(url, agent)]),
      MessageSubmitted(:final url, :final text) => _toChat(s, url, (session) => ClientCommandMessageSend(session: session, text: text)),
      CancelRequested(:final url) => _toChat(s, url, (session) => ClientCommandTurnCancel(session: session)),
      ApprovalAnswered(:final url, :final request, :final approve) => (
          _updateStatus(s, url, (x) => x.copyWith(approvals: [for (final a in x.approvals) if (a.request != request) a])),
          [
            SendCommand(
              url,
              ClientCommandApprovalRespond(request: request, decision: approve ? ApprovalDecision.approve : ApprovalDecision.deny),
            ),
          ],
        ),
      CommandFailed(:final message) => (s.copyWith(notice: Notice((s.notice?.id ?? 0) + 1, message)), const []),
      PageRequested(:final url, :final page) => _withPage(s, url, (p) => _navigate(url, p, page)),
      PageBack(:final url) => _withPage(s, url, (p) {
          if (p.back.isEmpty) return (p, const []);
          final target = p.back.last;
          return (
            p.copyWith(
              back: p.back.sublist(0, p.back.length - 1),
              forward: [...p.forward, if (p.page != null) p.page!.url],
              loading: target,
              error: null,
              appSite: null,
            ),
            [FetchPage(url, target)],
          );
        }),
      PageForward(:final url) => _withPage(s, url, (p) {
          if (p.forward.isEmpty) return (p, const []);
          final target = p.forward.last;
          return (
            p.copyWith(
              forward: p.forward.sublist(0, p.forward.length - 1),
              back: [...p.back, if (p.page != null) p.page!.url],
              loading: target,
              error: null,
              appSite: null,
            ),
            [FetchPage(url, target)],
          );
        }),
      PageReloadRequested(:final url) => _withPage(
          s,
          url,
          (p) => p.page == null ? (p, const []) : (p.copyWith(appSite: null), [FetchPage(url, p.page!.url)]),
        ),
      PageLoaded(:final url, :final requested, :final page) => _withPage(s, url, (p) {
          final wanted = p.loading == requested || (p.loading == null && p.page != null && samePage(p.page!.url, requested));
          return wanted ? (p.copyWith(page: page, loading: null, error: null), const []) : (p, const []);
        }),
      PageFailed(:final url, :final requested, :final error) => _withPage(
          s,
          url,
          (p) => p.loading == requested ? (p.copyWith(loading: null, error: '$requested: $error'), const []) : (p, const []),
        ),
      FollowToggled(:final url) => _withPage(s, url, (p) => (p.copyWith(follow: !p.follow), const [])),
      AppRequested(:final url, :final site) => _withPage(s, url, (p) => (p.copyWith(appSite: site, error: null), const [])),
      ReaderClosed(:final url) => _withPage(s, url, (p) => (PageState(follow: p.follow), const [])),
      SitesLoaded(:final url, :final sites) => (_updateStatus(s, url, (x) => x.copyWith(sites: sites)), const []),
      NoticeShown(:final text) => (s.copyWith(notice: Notice((s.notice?.id ?? 0) + 1, text)), const []),
    };

/// Pairing again with a known server replaces its token.
(AppState, List<AppEffect>) _paired(AppState s, ServerEntry server, ServerInfo info) {
  final known = s.servers.any((x) => x.url == server.url);
  final servers = known ? [for (final x in s.servers) x.url == server.url ? server : x] : [...s.servers, server];
  return (
    s.copyWith(
      servers: servers,
      status: {...s.status, server.url: ServerStatus(info: info)},
      active: server.url,
      pairing: const PairingState(),
    ),
    [SaveServers(servers), Connect(server)],
  );
}

(AppState, List<AppEffect>) _removed(AppState s, String url) {
  final servers = [for (final x in s.servers) if (x.url != url) x];
  final active = s.active == url ? servers.firstOrNull?.url : s.active;
  return (
    s.copyWith(servers: servers, status: {...s.status}..remove(url), active: active),
    [Disconnect(url), SaveServers(servers)],
  );
}

/// Keeps the stored name in step with the server's.
(AppState, List<AppEffect>) _info(AppState s, String url, ServerInfo info) {
  final next = _updateStatus(s, url, (x) => x.copyWith(info: info));
  final entry = s.servers.where((x) => x.url == url).firstOrNull;
  if (entry == null || entry.name == info.name) return (next, const []);
  final servers = [for (final x in s.servers) x.url == url ? x.withName(info.name) : x];
  return (next.copyWith(servers: servers), [SaveServers(servers)]);
}

/// Coming online: the server re-sends open approvals, and the open chat catches up from its last seq.
(AppState, List<AppEffect>) _link(AppState s, String url, LinkState link, String? error, DateTime? retryAt) {
  final online = link == LinkState.online;
  final next = _updateStatus(
    s,
    url,
    (x) => x.copyWith(link: link, error: error, retryAt: retryAt, approvals: online ? const [] : null),
  );
  final chat = next.status[url]?.chat;
  if (!online) return (next, const []);
  return (
    next,
    [
      FetchSites(url),
      if (chat != null) SendCommand(url, ClientCommandSessionAttach(session: chat.session, since: chat.lastSeq)),
    ],
  );
}

(AppState, List<AppEffect>) _open(AppState s, String url, String session) {
  final current = s.status[url]?.chat;
  if (current?.session == session) return (s, const []);
  return (
    _updateStatus(s, url, (x) => x.copyWith(chat: ChatState(session: session, liveAfter: x.session(session)?.lastSeq ?? 0))),
    [
      if (current != null) SendCommand(url, ClientCommandSessionDetach(session: current.session)),
      SendCommand(url, ClientCommandSessionAttach(session: session)),
    ],
  );
}

(AppState, List<AppEffect>) _close(AppState s, String url) {
  final current = s.status[url]?.chat;
  if (current == null) return (s, const []);
  return (
    _updateStatus(s, url, (x) => x.copyWith(chat: null)),
    [SendCommand(url, ClientCommandSessionDetach(session: current.session))],
  );
}

/// A command for the open session.
(AppState, List<AppEffect>) _toChat(AppState s, String url, ClientCommand Function(String session) command) {
  final chat = s.status[url]?.chat;
  return chat == null ? (s, const []) : (s, [SendCommand(url, command(chat.session))]);
}

(AppState, List<AppEffect>) _serverEvent(AppState s, String url, ServerMessage message) {
  final next = _event(s, url, message);
  final status = s.status[url];
  if (status == null) return (next, const []);
  final page = status.page;
  return switch (message) {
    // The open page changed on disk: reload it in place.
    ServerMessagePageChanged(url: final changed)
        when page.page != null && page.loading == null && samePage(page.page!.url, changed) =>
      (next, [FetchPage(url, page.page!.url)]),
    // Follow the agent, live events of the open session only.
    ServerMessageAgentNavigated(:final session, :final seq, url: final target)
        when page.follow && status.chat?.session == session && seq > status.chat!.liveAfter =>
      _withPage(next, url, (p) => _navigate(url, p, target)),
    _ => (next, const []),
  };
}

(PageState, List<AppEffect>) _navigate(String url, PageState p, String target) {
  final current = p.page?.url;
  final push = current != null && !samePage(current, target);
  return (
    p.copyWith(
      loading: target,
      error: null,
      appSite: null,
      back: push ? [...p.back, current] : null,
      forward: push ? const [] : null,
    ),
    [FetchPage(url, target)],
  );
}

(AppState, List<AppEffect>) _withPage(AppState s, String url, (PageState, List<AppEffect>) Function(PageState) update) {
  final status = s.status[url];
  if (status == null) return (s, const []);
  final (page, effects) = update(status.page);
  return (s.copyWith(status: {...s.status, url: status.copyWith(page: page)}), effects);
}

AppState _event(AppState s, String url, ServerMessage message) => switch (message) {
      ServerMessageOk(result: CommandResultSessions(:final sessions)) =>
        _updateStatus(s, url, (x) => x.copyWith(sessions: _newestFirst(sessions))),
      ServerMessageSessionUpdated(:final session) => _updateStatus(
          s,
          url,
          (x) => x.copyWith(sessions: _newestFirst([session, for (final y in x.sessions) if (y.id != session.id) y])),
        ),
      ServerMessageApprovalRequest(:final request) => _updateStatus(
          s,
          url,
          (x) => x.approvals.any((a) => a.request == request) ? x : x.copyWith(approvals: [...x.approvals, message]),
        ),
      ServerMessageApprovalResolved(:final request) =>
        _updateStatus(s, url, (x) => x.copyWith(approvals: [for (final a in x.approvals) if (a.request != request) a])),
      _ when sessionEvent(message) != null => _updateStatus(s, url, (x) {
          final chat = x.chat;
          if (chat == null) return x;
          final next = applyChatEvent(chat, message);
          return identical(next, chat) ? x : x.copyWith(chat: next);
        }),
      _ => s,
    };

List<SessionInfo> _newestFirst(List<SessionInfo> sessions) => [...sessions]..sort((a, b) => b.updated.compareTo(a.updated));

AppState _updateStatus(AppState s, String url, ServerStatus Function(ServerStatus) update) {
  final current = s.status[url];
  if (current == null) return s; // removed meanwhile
  return s.copyWith(status: {...s.status, url: update(current)});
}
