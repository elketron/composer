import 'package:composer_protocol/composer_protocol.dart';

import 'actions.dart';
import 'approval_text.dart';
import 'apps.dart';
import 'app_state.dart';
import 'chat_events.dart';
import 'chat_rows.dart';
import 'chat_state.dart';
import 'effects.dart';
import 'live_view_size.dart';
import 'page_state.dart';
import 'run_kind.dart';
import 'site_url.dart';
import 'server_entry.dart';
import 'server_status.dart';
import 'types.dart';

/// The app's reducer: pure, so it is tested without IO.
(AppState, List<AppEffect>) reduce(AppState s, AppAction action) => switch (action) {
      AppStarted() => (s, const [LoadServers(), LoadPreferences()]),
      ForegroundChanged(foreground: true) => (
          s.copyWith(foreground: true),
          [for (final server in s.servers) if (s.statusOf(server.url).link == LinkState.offline) Reconnect(server.url)],
        ),
      ForegroundChanged(foreground: false) => _pendingApprovals(s.copyWith(foreground: false)),
      ServersLoaded(:final servers) => (
          s.copyWith(
            loaded: true,
            servers: servers,
            status: {for (final server in servers) server.url: ServerStatus.initial},
            active: servers.firstOrNull?.url,
          ),
          [for (final server in servers) Connect(server), KeepAlive(servers.length)],
        ),
      ServerSelected(:final url) => s.servers.any((x) => x.url == url) ? (s.copyWith(active: url), const []) : (s, const []),
      PairingReset() => (s.copyWith(pairing: const PairingState()), const []),
      PairRequested(:final url, :final code, :final deviceName) => (
          s.copyWith(pairing: const PairingState(busy: true)),
          [Pair(url: url, code: code, deviceName: deviceName)],
        ),
      PairFailed(:final message) => (s.copyWith(pairing: PairingState(error: message)), const []),
      PairSucceeded(:final server, :final info) => _paired(
          s,
          server.withName(info.name),
          info,
        ),
      ServerRemoved(:final url) => _removed(s, url),
      ReconnectRequested(:final url) => (s, [Reconnect(url)]),
      LinkChanged(:final url, :final link, :final error, :final retryAt) => _link(s, url, link, error, retryAt),
      InfoReceived(:final url, :final info) => _info(s, url, info),
      ServerEvent(:final url, :final message) => _serverEvent(s, url, message),
      SessionOpened(:final url, :final session) => _open(s, url, session),
      OlderRequested(:final url, :final session) => _older(s, url, session),
      HistoryLoaded(:final url, :final session, :final before, :final events, :final more) =>
        _historyLoaded(s, url, session, before, events, more),
      SessionClosed(:final url) => _close(s, url),
      ViewScrolled(:final url, :final session, :final atBottom) => (_scrolled(s, url, session, atBottom), const []),
      NewSessionRequested(:final url, :final kind, :final site, :final harness) =>
        (s, [CreateSession(url, kind, site: site, harness: kind == RunKind.agentic ? harness : null)]),
      SessionDeleteRequested(:final url, :final session) => _delete(s, url, session),
      ProcessesLoaded(:final url, :final session, :final processes) =>
        (_withChat(s, url, (c) => c.session == session ? c.copyWith(processes: processes) : c), const []),
      DeskLoaded(:final url, :final session, :final desk) =>
        (_withChat(s, url, (c) => c.session == session ? c.copyWith(desk: desk) : c), const []),
      AppOpened(:final url, :final name) => (s, _openApp(s, url, name)),
      ProcessStopRequested(:final url, :final terminal) => (s, [SendCommand(url, ClientCommandProcessStop(terminal: terminal))]),
      SiteChosen(:final url, :final site, :final session) => _siteChosen(s, url, site, session),
      PreferencesLoaded(:final liveViewSize) => (s.copyWith(liveViewSize: liveViewSize), const []),
      LiveViewResized(:final size) => _liveViewSized(s, size),
      LiveViewFrameSized(:final frame) => (frame == s.liveFrame ? s : s.copyWith(liveFrame: frame), const []),
      LiveViewStepped() => _liveViewSized(s, s.liveViewSize.next),
      MessageSubmitted(:final url, :final text) => _submit(s, url, text),
      FilesPicked(:final url, :final files) => _picked(s, url, files),
      UploadProgressed(:final url, :final id, :final progress) => (
          _withUpload(s, url, id, (u) => u.status == UploadStatus.uploading ? u.copyWith(progress: progress) : u),
          const [],
        ),
      UploadFinished(:final url, :final id, :final attachment, :final error) => (
          _withUpload(
            s,
            url,
            id,
            (u) => attachment != null
                ? u.copyWith(status: UploadStatus.ready, attachment: attachment, progress: 1)
                : u.copyWith(status: UploadStatus.failed, error: error ?? 'upload failed'),
          ),
          const [],
        ),
      UploadRetried(:final url, :final id) => _retried(s, url, id),
      UploadRemoved(:final url, :final id) => (
          _withChat(s, url, (c) => c.copyWith(uploads: [for (final u in c.uploads) if (u.id != id) u])),
          const [],
        ),
      DraftChanged(:final url, :final drafting) => (
          _updateStatus(s, url, (x) => x.chat == null ? x : x.copyWith(chat: x.chat!.copyWith(drafting: drafting))),
          const [],
        ),
      ModelsRequested(:final url) => (s, [SendCommand(url, const ClientCommandModelList())]),
      ModelChosen(:final url, :final model) => _toChat(s, url, (session) => ClientCommandModelSet(session: session, model: model)),
      CompactRequested(:final url) => _toChat(s, url, (session) => ClientCommandSessionCompact(session: session)),
      TabSelected(:final tab) => (s.copyWith(tab: tab), const []),
      SearchChanged(:final url, :final query) => (
          _updateStatus(s, url, (x) => x.copyWith(search: PageSearch(query: query, hits: query.trim().isEmpty ? const [] : null))),
          [if (query.trim().isNotEmpty) SearchPages(url, query)],
        ),
      SearchLoaded(:final url, :final query, :final hits, :final error) => (
          // A late answer for an older query is dropped.
          _updateStatus(s, url, (x) => x.search.query == query ? x.copyWith(search: PageSearch(query: query, hits: hits, error: error)) : x),
          const [],
        ),
      // What it had seen: a turn that started since (a queued message) keeps going.
      CancelRequested(:final url) => _toChat(
          s,
          url,
          (session) => ClientCommandTurnCancel(
                session: session,
                seen: switch (s.status[url]?.chat?.lastSeq) { final seq? when seq > 0 => seq, _ => null },
              ),
        ),
      ApprovalAnswered(:final url, :final request, :final decision) => (
          _updateStatus(s, url, (x) => x.copyWith(approvals: [for (final a in x.approvals) if (a.request != request) a])),
          [
            SendCommand(
              url,
              ClientCommandApprovalRespond(request: request, decision: decision),
            ),
            CancelNotification('approval:$request'),
          ],
        ),
      CommandFailed(:final message) => (s.copyWith(notice: Notice((s.notice?.id ?? 0) + 1, message)), const []),
      PageRequested(:final url, :final page) => _withPage(_reader(s, url, ReaderTab.page), url, (p) => _navigate(url, p, page)),
      PageBack(:final url) => _withPage(s, url, (p) {
          if (p.back.isEmpty) return (p, const []);
          final target = p.back.last;
          return (
            p.copyWith(
              back: p.back.sublist(0, p.back.length - 1),
              forward: [...p.forward, if (p.page != null) p.page!.url],
              loading: target,
              error: null,
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
            ),
            [FetchPage(url, target)],
          );
        }),
      PageReloadRequested(:final url) => _withPage(
          s,
          url,
          (p) => p.page == null ? (p, const []) : (p, [FetchPage(url, p.page!.url)]),
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
      ReaderClosed(:final url) => _withPage(_reader(s, url, null), url, (p) => (PageState(follow: p.follow), const [])),
      CatalogRequested(:final url) => (s, [FetchCatalog(url)]),
      SitesLoaded(:final url, :final sites) => (_updateStatus(s, url, (x) => x.copyWith(sites: sites)), const []),
      NoticeShown(:final text) => (s.copyWith(notice: Notice((s.notice?.id ?? 0) + 1, text)), const []),
    };

/// The open chat closes first (its page goes), then the server deletes it.
(AppState, List<AppEffect>) _delete(AppState s, String url, String session) {
  final open = s.status[url]?.chat?.session == session;
  final (closed, effects) = open ? _close(s, url) : (s, const <AppEffect>[]);
  return (closed, [...effects, SendCommand(url, ClientCommandSessionDelete(session: session))]);
}

/// `site.set` for the session (default: the open chat's); the reply's sentence comes back as a notice.
(AppState, List<AppEffect>) _siteChosen(AppState s, String url, String? site, String? session) {
  final target = session ?? s.status[url]?.chat?.session;
  if (target == null) return (s, const []);
  return (s, [SendCommand(url, ClientCommandSiteSet(session: target, site: site))]);
}

(AppState, List<AppEffect>) _liveViewSized(AppState s, LiveViewSize size) =>
    size == s.liveViewSize ? (s, const []) : (s.copyWith(liveViewSize: size), [SaveLiveViewSize(size)]);

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
    [SaveServers(servers), Connect(server), KeepAlive(servers.length)],
  );
}

(AppState, List<AppEffect>) _removed(AppState s, String url) {
  final servers = [for (final x in s.servers) if (x.url != url) x];
  final active = s.active == url ? servers.firstOrNull?.url : s.active;
  return (
    s.copyWith(servers: servers, status: {...s.status}..remove(url), active: active),
    [Disconnect(url), SaveServers(servers), KeepAlive(servers.length)],
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
      FetchCatalog(url),
      if (chat != null) ...[_reattach(url, chat), FetchProcesses(url, chat.session), _fetchDesk(url, chat.session)],
    ],
  );
}

/// After a reconnect: what came since, or (its history never arrived) its last messages again.
AppEffect _reattach(String url, ChatState chat) => chat.loaded
    ? SendCommand(url, ClientCommandSessionAttach(session: chat.session, since: chat.lastSeq))
    : FetchHistory(url, chat.session, null);

AppEffect _fetchDesk(String url, String session) => DeskCommand(url, session, ClientCommandDeskGet(session: session));

/// App [name] by the user's hand: put on the open session's desk at its page when it is not there
/// (not focused: another client's shown tab stays), focused when the agent put it there and it is
/// still unseen. Already there and seen: nothing to tell.
List<AppEffect> _openApp(AppState s, String url, String name) {
  final chat = s.status[url]?.chat;
  if (chat == null) return const [];
  final session = chat.session;
  final entry = chat.desk.tabs.where((e) => e.id == appTabId(name)).firstOrNull;
  if (entry == null) {
    return [DeskCommand(url, session, ClientCommandDeskOpen(session: session, tab: DeskTabApp(name: name, path: '')))];
  }
  if (entry.unseen) return [DeskCommand(url, session, ClientCommandDeskFocus(session: session, tab: entry.id))];
  return const [];
}

/// The messages before the oldest shown, once at a time.
(AppState, List<AppEffect>) _older(AppState s, String url, String session) {
  final status = s.status[url];
  final chat = status?.chat;
  if (chat == null || chat.session != session || !chat.loaded || !chat.more || chat.loadingOlder) return (s, const []);
  return (
    _updateStatus(s, url, (x) => x.copyWith(chat: chat.copyWith(loadingOlder: true))),
    [FetchHistory(url, session, chat.firstSeq)],
  );
}

/// The view came back to the newest line, or scrolled up off it: back down, what piled up is seen.
AppState _scrolled(AppState s, String url, String session, bool atBottom) {
  ChatState? mark(ChatState? c) => c == null || c.session != session ? c : c.copyWith(atBottom: atBottom, newSince: atBottom ? 0 : null);
  return _updateStatus(s, url, (x) => x.copyWith(chat: mark(x.chat)));
}

/// A chat's last messages (then it attaches for what follows), or older ones put in front.
(AppState, List<AppEffect>) _historyLoaded(AppState s, String url, String session, int? before, List<ServerMessage>? events, bool more) {
  var attach = false;
  int? firstOf(List<ServerMessage> events) => events.map(sessionEvent).firstWhere((e) => e != null, orElse: () => null)?.$2;
  ChatState apply(ChatState c) {
    if (c.session != session) return c;
    if (before == null) {
      if (c.loaded) return c;
      attach = true;
      if (events == null) return c.copyWith(loaded: true);
      final loaded = events.fold(c, applyChatEvent);
      return loaded.copyWith(loaded: true, more: more, firstSeq: firstOf(events) ?? 0);
    }
    if (!c.loadingOlder || before != c.firstSeq) return c;
    if (events == null || events.isEmpty) return c.copyWith(loadingOlder: false, more: false);
    final older = events.fold(ChatState(session: session), applyChatEvent).items;
    final items = [...older, ...c.items];
    return c.copyWith(
      items: items,
      loadingOlder: false,
      more: more,
      firstSeq: firstOf(events) ?? c.firstSeq,
      rowBase: c.rowBase + chatRows(items).length - chatRows(c.items).length,
    );
  }

  final next = _updateStatus(
    s,
    url,
    (x) => x.copyWith(chat: x.chat == null ? null : apply(x.chat!)),
  );
  final chat = next.status[url]?.chat;
  final since = chat?.session == session ? chat?.lastSeq : null;
  // Since the last of them: nothing in between is lost (`events` null: all of it).
  return (
    next,
    [
      if (attach) ...[
        SendCommand(url, ClientCommandSessionAttach(session: session, since: events == null ? null : since)),
        FetchProcesses(url, session),
        _fetchDesk(url, session),
      ],
    ],
  );
}

(AppState, List<AppEffect>) _open(AppState s, String url, String session) {
  final current = s.status[url]?.chat;
  if (current?.session == session) return (s, const []);
  return (
    _updateStatus(
      s,
      url,
      (x) => x.copyWith(
        chat: ChatState(session: session, liveAfter: x.session(session)?.lastSeq ?? 0),
        unread: x.unread.contains(session) ? ({...x.unread}..remove(session)) : null,
      ),
    ),
    [
      if (current != null) SendCommand(url, ClientCommandSessionDetach(session: current.session)),
      FetchHistory(url, session, null),
    ],
  );
}

/// The reader screen shows `tab` (null: closed).
AppState _reader(AppState s, String url, ReaderTab? tab) => _updateStatus(s, url, (x) => x.copyWith(reader: tab));

(AppState, List<AppEffect>) _close(AppState s, String url) {
  final current = s.status[url]?.chat;
  if (current == null) return (s, const []);
  return (
    _updateStatus(s, url, (x) => x.copyWith(chat: null)),
    [SendCommand(url, ClientCommandSessionDetach(session: current.session))],
  );
}

/// A command for the open session.
/// Sends the message with its uploaded files; not while one is still uploading or failed.
(AppState, List<AppEffect>) _submit(AppState s, String url, String text) {
  final chat = s.status[url]?.chat;
  if (chat == null || chat.uploads.any((u) => u.status != UploadStatus.ready)) return (s, const []);
  final attachments = [for (final u in chat.uploads) u.attachment!];
  if (text.trim().isEmpty && attachments.isEmpty) return (s, const []);
  return (
    _withChat(s, url, (c) => c.copyWith(uploads: const [])),
    [SendCommand(url, ClientCommandMessageSend(session: chat.session, text: text, attachments: attachments))],
  );
}

(AppState, List<AppEffect>) _picked(AppState s, String url, List<PickedFile> files) {
  final chat = s.status[url]?.chat;
  if (chat == null || files.isEmpty) return (s, const []);
  final added = [for (var i = 0; i < files.length; i++) Upload(id: chat.nextUpload + i, file: files[i])];
  return (
    _withChat(s, url, (c) => c.copyWith(uploads: [...c.uploads, ...added], nextUpload: c.nextUpload + files.length)),
    [for (final u in added) UploadAttachment(url, chat.session, u.id, u.file)],
  );
}

(AppState, List<AppEffect>) _retried(AppState s, String url, int id) {
  final chat = s.status[url]?.chat;
  final upload = chat?.uploads.where((u) => u.id == id).firstOrNull;
  if (chat == null || upload == null || upload.status != UploadStatus.failed) return (s, const []);
  return (
    _withUpload(s, url, id, (u) => u.copyWith(status: UploadStatus.uploading, progress: 0)),
    [UploadAttachment(url, chat.session, id, upload.file)],
  );
}

AppState _withChat(AppState s, String url, ChatState Function(ChatState chat) update) =>
    _updateStatus(s, url, (x) => x.chat == null ? x : x.copyWith(chat: update(x.chat!)));

AppState _withUpload(AppState s, String url, int id, Upload Function(Upload upload) update) =>
    _withChat(s, url, (c) => c.copyWith(uploads: [for (final u in c.uploads) u.id == id ? update(u) : u]));

(AppState, List<AppEffect>) _toChat(AppState s, String url, ClientCommand Function(String session) command) {
  final chat = s.status[url]?.chat;
  return chat == null ? (s, const []) : (s, [SendCommand(url, command(chat.session))]);
}

(AppState, List<AppEffect>) _serverEvent(AppState s, String url, ServerMessage message) {
  final (next, notes) = _notifications(s, _event(s, url, message), url, message);
  final (state, effects) = _pageEffects(s, next, url, message);
  return (state, [...notes, ...effects]);
}

(AppState, List<AppEffect>) _pageEffects(AppState s, AppState next, String url, ServerMessage message) {
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
      // A closed reader opens on the page.
      _withPage(status.reader == null ? _reader(next, url, ReaderTab.page) : next, url, (p) => _navigate(url, p, target)),
    _ => (next, const []),
  };
}

/// What deserves a system notification: approvals while the app is in the background, and
/// finished turns you are not looking at.
(AppState, List<AppEffect>) _notifications(AppState before, AppState s, String url, ServerMessage message) {
  final status = before.status[url];
  if (status == null) return (s, const []);
  final server = s.servers.where((x) => x.url == url).firstOrNull?.name ?? url;
  bool viewing(String? session) =>
      s.foreground && s.active == url && session != null && s.status[url]?.chat?.session == session;
  switch (message) {
    case ServerMessageApprovalRequest(:final request) when !s.foreground && !s.notifiedApprovals.contains(request):
      return (s.copyWith(notifiedApprovals: {...s.notifiedApprovals, request}), [_approvalNote(url, server, message)]);
    case ServerMessageApprovalResolved(:final request):
      return (s, [CancelNotification('approval:$request')]);
    case ServerMessageSessionUpdated(:final session) when session.state == TurnState.idle && !viewing(session.id):
      final previous = status.session(session.id);
      if (previous == null || previous.state == TurnState.idle) return (s, const []);
      return (
        s,
        [
          Notify(
            key: 'turn:${session.id}',
            title: '${sessionLabel(session)} #${session.id} finished',
            body: session.preview ?? 'on $server',
            url: url,
            session: session.id,
          ),
        ],
      );
    default:
      return (s, const []);
  }
}

Notify _approvalNote(String url, String server, ServerMessageApprovalRequest a) => Notify(
      key: 'approval:${a.request}',
      title: '${approvalQuestion(a)} ${approvalHeadline(a)}',
      body: 'Approval for #${a.session} on $server',
      url: url,
      session: a.session,
      approval: a.request,
    );

/// Going to the background: approvals still open get their notification now.
(AppState, List<AppEffect>) _pendingApprovals(AppState s) {
  final notes = [
    for (final server in s.servers)
      for (final a in s.statusOf(server.url).approvals)
        if (!s.notifiedApprovals.contains(a.request)) _approvalNote(server.url, server.name, a),
  ];
  if (notes.isEmpty) return (s, const []);
  return (s.copyWith(notifiedApprovals: {...s.notifiedApprovals, for (final n in notes) n.approval!}), notes);
}

(PageState, List<AppEffect>) _navigate(String url, PageState p, String target) {
  final current = p.page?.url;
  final push = current != null && !samePage(current, target);
  return (
    p.copyWith(
      loading: target,
      error: null,
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
      ServerMessageSessionUpdated(:final session) => _updateStatus(s, url, (x) {
          // A turn ended in a session that isn't open: it has something new for you.
          final finished = session.state == TurnState.idle &&
              (x.session(session.id)?.state ?? TurnState.idle) != TurnState.idle &&
              x.chat?.session != session.id;
          return x.copyWith(
            sessions: _newestFirst([session, for (final y in x.sessions) if (y.id != session.id) y]),
            unread: finished ? {...x.unread, session.id} : null,
          );
        }),
      ServerMessageSessionDeleted(:final session) => _updateStatus(
          s,
          url,
          (x) => x.copyWith(
            sessions: [for (final y in x.sessions) if (y.id != session) y],
            approvals: [for (final a in x.approvals) if (a.session != session) a],
            unread: x.unread.contains(session) ? ({...x.unread}..remove(session)) : null,
            chat: x.chat?.session == session ? null : x.chat,
          ),
        ),
      ServerMessageDeskChanged(:final session, :final desk) =>
        _withChat(s, url, (c) => c.session == session ? c.copyWith(desk: desk) : c),
      ServerMessageProcessesChanged(:final session, :final processes) =>
        _withChat(s, url, (c) => c.session == session ? c.copyWith(processes: processes) : c),
      ServerMessageOk(result: final CommandResultModels models) => _updateStatus(s, url, (x) => x.copyWith(models: models)),
      ServerMessageSlots(:final used) => _updateStatus(s, url, (x) => x.copyWith(slotsUsed: used)),
      ServerMessagePageChanged(url: final page, :final by) when by == ChangedBy.agent =>
        _updateStatus(s, url, (x) => x.copyWith(recent: [RecentPage(page, by), ...x.recent.where((r) => r.url != page)].take(12).toList())),
      ServerMessageApprovalRequest(:final request) => _updateStatus(
          s,
          url,
          (x) => x.approvals.any((a) => a.request == request) ? x : x.copyWith(approvals: [...x.approvals, message]),
        ),
      ServerMessageApprovalResolved(:final request) =>
        _updateStatus(s, url, (x) => x.copyWith(approvals: [for (final a in x.approvals) if (a.request != request) a])),
      _ when sessionEvent(message) != null => _updateStatus(s, url, (x) {
          final chat = x.chat;
          // Not before its history: the attach after it replays what came meanwhile.
          final nextChat = chat == null || !chat.loaded ? chat : applyChatEvent(chat, message);
          return identical(nextChat, chat) ? x : x.copyWith(chat: nextChat);
        }),
      _ => s,
    };

List<SessionInfo> _newestFirst(List<SessionInfo> sessions) => [...sessions]..sort((a, b) => b.updated.compareTo(a.updated));

AppState _updateStatus(AppState s, String url, ServerStatus Function(ServerStatus) update) {
  final current = s.status[url];
  if (current == null) return s; // removed meanwhile
  return s.copyWith(status: {...s.status, url: update(current)});
}
