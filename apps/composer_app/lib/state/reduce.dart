import 'package:composer_protocol/composer_protocol.dart';

import 'actions.dart';
import 'approval_text.dart';
import 'app_state.dart';
import 'chat_events.dart';
import 'chat_items.dart';
import 'chat_rows.dart';
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
      ForegroundChanged(foreground: true) => (
          s.copyWith(foreground: true),
          [for (final server in s.servers) if (s.statusOf(server.url).link == LinkState.offline) Reconnect(server.url)],
        ),
      ForegroundChanged(foreground: false) => _pendingApprovals(s.copyWith(foreground: false)),
      ServersLoaded(:final servers, :final storage) => (
          s.copyWith(
            loaded: true,
            storage: storage,
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
      PairSucceeded(:final server, :final info) => _paired(s, server.withName(info.name), info),
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
      NewSessionRequested(:final url, :final agent, :final project, :final text) =>
        (s, [CreateSession(url, agent, project: project, text: text)]),
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
      ProjectChosen(:final url, :final project) =>
        _toChat(s, url, (session) => ClientCommandProjectSet(session: session, path: project)),
      ModelsRequested(:final url) => (s, [SendCommand(url, const ClientCommandModelList())]),
      ModelChosen(:final url, :final model) => _toChat(s, url, (session) => ClientCommandModelSet(session: session, model: model)),
      HarnessesRequested(:final url, :final project) =>
        (s, [SendCommand(url, ClientCommandHarnessList(project: project))]),
      HarnessToggled(:final url, :final project, :final entry, :final on) =>
        (s, [SendCommand(url, ClientCommandHarnessSet(project: project, entry: entry, on: on))]),
      ReviewRequested(:final url, :final ticket) =>
        _ticketCommand(s, url, ticket, ClientCommandTicketChanges(ticket: ticket)),
      TicketMergeRequested(:final url, :final ticket) =>
        _ticketCommand(s, url, ticket, ClientCommandTicketMerge(ticket: ticket)),
      TicketDiscardRequested(:final url, :final ticket) =>
        _ticketCommand(s, url, ticket, ClientCommandTicketDiscard(ticket: ticket)),
      TicketReplied(:final url, :final ticket, :final result, :final error) => _ticketReplied(s, url, ticket, result, error),
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
      AgentsLoaded(:final url, :final agents) => (_updateStatus(s, url, (x) => x.copyWith(agents: agents)), const []),
      ProjectsLoaded(:final url, :final projects) => (_updateStatus(s, url, (x) => x.copyWith(projects: projects)), const []),
      CancelRequested(:final url) => _toChat(s, url, (session) => ClientCommandTurnCancel(session: session)),
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
      PageRequested(:final url, :final page) => _withPage(_side(s, url, SideTab.page), url, (p) => _navigate(url, p, page)),
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
      AppRequested(:final url, :final site) =>
        _withPage(_side(s, url, SideTab.app), url, (p) => (p.copyWith(appSite: site, error: null), const [])),
      ReaderClosed(:final url) => _withPage(_side(s, url, null), url, (p) => (PageState(follow: p.follow), const [])),
      SideTabSelected(:final url, :final tab, :final ticket, :final session, :final project) => tab == SideTab.project && project != null
          ? (
              _updateStatus(s, url, (x) => x.copyWith(side: tab, sideProject: project)),
              [SendCommand(url, ClientCommandProjectOverview(project: project))],
            )
          : _sideTab(s, url, tab, ticket, session),
      ProjectSearchChanged(:final url, :final query) => (
          _updateStatus(s, url, (x) => x.copyWith(projectSearch: ProjectSearch(query: query, hits: query.trim().isEmpty ? const [] : null))),
          [if (query.trim().isNotEmpty) SearchProjects(url, query)],
        ),
      ProjectsFound(:final url, :final query, :final hits, :final error) => (
          // A late answer for an older query is dropped.
          _updateStatus(
            s,
            url,
            (x) => x.projectSearch.query == query ? x.copyWith(projectSearch: ProjectSearch(query: query, hits: hits, error: error)) : x,
          ),
          const [],
        ),
      ProjectOverviewRequested(:final url, :final project) => (s, [SendCommand(url, ClientCommandProjectOverview(project: project))]),
      CatalogRequested(:final url) => (s, [FetchCatalog(url)]),
      PipelinesRequested(:final url) => (s, [SendCommand(url, const ClientCommandPipelineList())]),
      WorkCreateRequested(:final url, :final pipeline, :final name, :final project, :final brief, :final template) => (
          _updateStatus(s, url, (x) => x.copyWith(newWork: const NewWork(busy: true))),
          [CreateWork(url, ClientCommandWorkNew(pipeline: pipeline, name: name, project: project, brief: brief), template: template)],
        ),
      WorkReplied(:final url, :final work, :final error) => (
          _updateStatus(s, url, (x) => x.copyWith(newWork: NewWork(error: error, created: work?.url))),
          // The new work (and maybe pipeline) shows in the lists.
          [if (work != null) SendCommand(url, const ClientCommandPipelineList())],
        ),
      WorkRunRequested(:final url, :final work, :final stage, :final project) => (
          s,
          [RunWork(url, ClientCommandWorkRun(work: work, stage: stage, session: s.status[url]?.chat?.session), project: project)],
        ),
      NewWorkReset(:final url) => (_updateStatus(s, url, (x) => x.copyWith(newWork: const NewWork())), const []),
      ScopeChanged(:final url, :final project) => (_updateStatus(s, url, (x) => x.copyWith(scope: project)), const []),
      SideClosed(:final url) => (_side(s, url, null), const []),
      SessionsToggled() => _layout(s, s.layout.copyWith(sessionsCollapsed: !s.layout.sessionsCollapsed)),
      SideResized(:final width) => _layout(s, s.layout.copyWith(sideWidth: width)),
      LayoutLoaded(:final layout) => (s.copyWith(layout: layout), const []),
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
  final chat = next.status[url]?.chat, side = next.status[url]?.sideChat;
  if (!online) return (next, const []);
  return (
    next,
    [
      FetchCatalog(url),
      // Works waiting at a gate (Needs you).
      SendCommand(url, const ClientCommandPipelineList()),
      if (chat != null) _reattach(url, chat),
      if (side != null && side.session != chat?.session) _reattach(url, side),
    ],
  );
}

/// After a reconnect: what came since, or (its history never arrived) its last messages again.
AppEffect _reattach(String url, ChatState chat) => chat.loaded
    ? SendCommand(url, ClientCommandSessionAttach(session: chat.session, since: chat.lastSeq))
    : FetchHistory(url, chat.session, null);

/// The messages before the oldest shown, once at a time.
(AppState, List<AppEffect>) _older(AppState s, String url, String session) {
  final status = s.status[url];
  final chat = [status?.chat, status?.sideChat].firstWhere((c) => c?.session == session, orElse: () => null);
  if (chat == null || !chat.loaded || !chat.more || chat.loadingOlder) return (s, const []);
  ChatState mark(ChatState c) => c.session == session ? c.copyWith(loadingOlder: true) : c;
  return (
    _updateStatus(s, url, (x) => x.copyWith(chat: x.chat == null ? null : mark(x.chat!), sideChat: x.sideChat == null ? null : mark(x.sideChat!))),
    [FetchHistory(url, session, chat.firstSeq)],
  );
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
    (x) => x.copyWith(chat: x.chat == null ? null : apply(x.chat!), sideChat: x.sideChat == null ? null : apply(x.sideChat!)),
  );
  final since = [next.status[url]?.chat, next.status[url]?.sideChat].firstWhere((c) => c?.session == session, orElse: () => null)?.lastSeq;
  // Since the last of them: nothing in between is lost (`events` null: all of it).
  return (next, [if (attach) SendCommand(url, ClientCommandSessionAttach(session: session, since: events == null ? null : since))]);
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
        opened: true,
      ),
    ),
    [
      if (current != null && current.session != s.status[url]?.sideChat?.session)
        SendCommand(url, ClientCommandSessionDetach(session: current.session)),
      FetchHistory(url, session, null),
    ],
  );
}

/// The side pane shows `tab` (null: closed).
AppState _side(AppState s, String url, SideTab? tab) => _updateStatus(s, url, (x) => x.copyWith(side: tab));

(AppState, List<AppEffect>) _sideTab(AppState s, String url, SideTab tab, String? ticket, String? session) {
  final next = _updateStatus(s, url, (x) => x.copyWith(side: tab, sideTicket: ticket ?? x.sideTicket));
  // A ticket's changes come fresh each time they're shown.
  if (tab == SideTab.changes && ticket != null) {
    return _ticketCommand(next, url, ticket, ClientCommandTicketChanges(ticket: ticket));
  }
  final status = next.status[url];
  final old = status?.sideChat?.session;
  if (tab != SideTab.worker || session == null || status == null || old == session) return (next, const []);
  // Attached as well (its last messages first; the open chat skips what it has seen), and the
  // one it replaces let go unless the open chat still shows it.
  return (
    _updateStatus(next, url, (x) => x.copyWith(sideChat: ChatState(session: session, liveAfter: x.session(session)?.lastSeq ?? 0))),
    [
      if (old != null && old != status.chat?.session) SendCommand(url, ClientCommandSessionDetach(session: old)),
      FetchHistory(url, session, null),
    ],
  );
}

(AppState, List<AppEffect>) _layout(AppState s, LayoutPrefs layout) => (s.copyWith(layout: layout), [SaveLayout(layout)]);

(AppState, List<AppEffect>) _close(AppState s, String url) {
  final current = s.status[url]?.chat;
  if (current == null) return (s, const []);
  return (
    _updateStatus(s, url, (x) => x.copyWith(chat: null)),
    [if (current.session != s.status[url]?.sideChat?.session) SendCommand(url, ClientCommandSessionDetach(session: current.session))],
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
  final (handedBack, switches) = _handBack(_event(s, url, message), url, message);
  final (next, notes) = _notifications(s, handedBack, url, message);
  final (paged, effects) = _pageEffects(s, next, url, message);
  final (state, reviews) = _reviewEffects(paged, url, message);
  // A stage ended: the works (Needs you, the project view) are fetched again.
  final works = message is ServerMessageStageEnded ? [SendCommand(url, const ClientCommandPipelineList())] : const <AppEffect>[];
  return (state, [...switches, ...notes, ...effects, ...reviews, ...works]);
}

/// Tools after which a ticket's files may differ.
const _changingTools = {'Edit', 'MultiEdit', 'Write', 'NotebookEdit', 'Bash'};

/// An open review follows its worker: refreshed after it changes files (live events of the
/// open chat) and when its run ends.
(AppState, List<AppEffect>) _reviewEffects(AppState s, String url, ServerMessage message) {
  final status = s.status[url];
  if (status == null) return (s, const []);
  final ticket = switch (message) {
    ServerMessageToolResult(:final session, :final seq, :final callId)
        when status.chat?.session == session &&
            seq > status.chat!.liveAfter &&
            status.chat!.items.any((i) => i is ToolItem && i.callId == callId && _changingTools.contains(i.name)) =>
      status.session(session)?.ticket,
    ServerMessageWorkerStatus(:final ticket, :final state) when stopped(state) => ticket,
    _ => null,
  };
  final review = ticket == null ? null : status.reviews[ticket];
  if (review == null || review.busy || review.merged != null || review.discarded) return (s, const []);
  return _ticketCommand(s, url, ticket!, ClientCommandTicketChanges(ticket: ticket));
}

/// A worker that ended its run: finished, failed, or paused until later.
bool stopped(WorkerState state) => state == WorkerState.done || state == WorkerState.failed || state == WorkerState.paused;

/// The worker you were watching finished: back to its orchestrator, which gets the result
/// next. Not while you're writing a message.
(AppState, List<AppEffect>) _handBack(AppState s, String url, ServerMessage message) {
  if (message case ServerMessageWorkerStatus(:final worker, :final ticket, :final parent, :final state)
      when stopped(state)) {
    final chat = s.status[url]?.chat;
    if (chat == null || chat.session != worker || chat.drafting) return (s, const []);
    final (opened, effects) = _open(s, url, parent);
    final what = switch (state) { WorkerState.done => 'finished', WorkerState.paused => 'paused', _ => 'failed' };
    return (opened.copyWith(notice: Notice((s.notice?.id ?? 0) + 1, 'Worker #$worker $what ticket $ticket: back to the orchestrator')), effects);
  }
  return (s, const []);
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
      // A closed side pane opens on the page; another tab stays up.
      _withPage(status.side == null ? _side(next, url, SideTab.page) : next, url, (p) => _navigate(url, p, target)),
    _ => (next, const []),
  };
}

/// What deserves a system notification: approvals while the app is in the background,
/// finished turns and workers you are not looking at, and reminders always.
(AppState, List<AppEffect>) _notifications(AppState before, AppState s, String url, ServerMessage message) {
  final status = before.status[url];
  if (status == null) return (s, const []);
  final server = s.servers.where((x) => x.url == url).firstOrNull?.name ?? url;
  // After this event: a worker handing back has already switched the chat to its orchestrator.
  bool viewing(String? session) =>
      s.foreground && s.active == url && session != null && s.status[url]?.chat?.session == session;
  switch (message) {
    case ServerMessageApprovalRequest(:final request) when !s.foreground && !s.notifiedApprovals.contains(request):
      return (s.copyWith(notifiedApprovals: {...s.notifiedApprovals, request}), [_approvalNote(url, server, message)]);
    case ServerMessageReminderDue(url: final page, :final title, :final due, :final late, :final session):
      return (
        s,
        [
          Notify(
            key: 'reminder:$page',
            title: title,
            body: '${late ? 'was due' : 'due'} $due · $server',
            url: url,
            session: session,
            page: page,
          ),
        ],
      );
    case ServerMessageApprovalResolved(:final request):
      return (s, [CancelNotification('approval:$request')]);
    // A stage waits for you at its gate, or failed: its pipeline's board (or the work's page).
    case ServerMessageStageEnded(:final work, :final title, :final stage, :final status, :final summary)
        when status == 'review' || status == 'failed':
      final site = work.replaceFirst('site://', '').split('/').first;
      final label = stage.replaceFirst('_', ' ');
      return (
        s,
        [
          Notify(
            key: 'stage:$work:$stage',
            title: status == 'review' ? '$title · $label ready for review' : '$title · $label failed',
            body: summary.isEmpty ? 'on $server' : summary,
            url: url,
            page: work,
            app: s.status[url]?.hasApp(site) ?? false ? site : null,
          ),
        ],
      );
    case ServerMessageWorkerStatus(:final worker, :final ticket, :final parent, state: final state)
        when stopped(state) && !viewing(parent):
      return (
        s,
        [
          Notify(
            key: 'worker:$worker',
            title: switch (state) {
              WorkerState.done => 'Worker finished ticket $ticket',
              WorkerState.paused => 'Worker paused on ticket $ticket (usage limit)',
              _ => 'Worker failed on ticket $ticket',
            },
            body: 'on $server',
            url: url,
            session: parent,
          ),
        ],
      );
    // Not a turn that answered a worker's approval: the orchestrator decided it, nothing for you.
    case ServerMessageSessionUpdated(:final session)
        when session.state == TurnState.idle &&
            session.ticket == null &&
            !viewing(session.id) &&
            !_workerApprovalAsk.hasMatch(session.preview ?? ''):
      final previous = status.session(session.id);
      if (previous == null || previous.state == TurnState.idle) return (s, const []);
      return (
        s,
        [
          Notify(
            key: 'turn:${session.id}',
            title: '${session.agent} #${session.id} finished',
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

(AppState, List<AppEffect>) _ticketCommand(AppState s, String url, String ticket, ClientCommand command) => (
      _updateStatus(s, url, (x) {
        final review = x.reviews[ticket] ?? const TicketReviewState();
        return x.copyWith(reviews: {...x.reviews, ticket: review.copyWith(busy: true)});
      }),
      [TicketCommand(url, ticket, command)],
    );

(AppState, List<AppEffect>) _ticketReplied(AppState s, String url, String ticket, CommandResult? result, String? error) {
  final review = (s.statusOf(url).reviews[ticket] ?? const TicketReviewState()).copyWith(busy: false, error: error);
  final next = switch (result) {
    CommandResultChanges(:final changes) => review.copyWith(changes: changes),
    CommandResultMerged(:final commit) => review.copyWith(merged: commit),
    CommandResultDone() => review.copyWith(discarded: true),
    _ => review,
  };
  final notice = switch (result) {
    CommandResultMerged(:final commit) => 'Ticket $ticket merged as $commit',
    CommandResultDone() => 'Ticket $ticket discarded',
    _ => null,
  };
  return (
    _updateStatus(
      notice == null ? s : s.copyWith(notice: Notice((s.notice?.id ?? 0) + 1, notice)),
      url,
      (x) => x.copyWith(reviews: {...x.reviews, ticket: next}),
    ),
    const [],
  );
}

/// The start of the message that asks an orchestrator to decide a worker's approval.
final _workerApprovalAsk = RegExp(r'^\[worker #\S+ on ticket \S+ asks to approve:');

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
              session.ticket == null &&
              x.chat?.session != session.id;
          return x.copyWith(
            sessions: _newestFirst([session, for (final y in x.sessions) if (y.id != session.id) y]),
            unread: finished ? {...x.unread, session.id} : null,
          );
        }),
      ServerMessageOk(result: CommandResultProject(:final overview)) =>
        _updateStatus(s, url, (x) => x.copyWith(overviews: {...x.overviews, overview.folder: overview})),
      ServerMessageOk(result: CommandResultPipelines(:final pipelines, :final templates, :final works)) =>
        _updateStatus(s, url, (x) => x.copyWith(pipelines: Pipelines(pipelines: pipelines, templates: templates, works: works))),
      ServerMessageOk(result: final CommandResultModels models) => _updateStatus(s, url, (x) => x.copyWith(models: models)),
      ServerMessageOk(result: CommandResultHarnesses(:final project, :final harnesses)) =>
        _updateStatus(s, url, (x) => x.copyWith(harnesses: {...x.harnesses, project: harnesses})),
      ServerMessageSlots(:final used) => _updateStatus(s, url, (x) => x.copyWith(slotsUsed: used)),
      ServerMessagePageChanged(url: final page, :final by) when by == ChangedBy.agent || by == ChangedBy.app =>
        _updateStatus(s, url, (x) => x.copyWith(recent: [RecentPage(page, by), ...x.recent.where((r) => r.url != page)].take(12).toList())),
      ServerMessageApprovalRequest(:final request) => _updateStatus(
          s,
          url,
          (x) => x.approvals.any((a) => a.request == request) ? x : x.copyWith(approvals: [...x.approvals, message]),
        ),
      ServerMessageApprovalResolved(:final request) =>
        _updateStatus(s, url, (x) => x.copyWith(approvals: [for (final a in x.approvals) if (a.request != request) a])),
      ServerMessageWorkerStatus(:final worker, :final state) => _updateStatus(s, url, (x) {
          final finished = stopped(state) && x.chat?.session != worker;
          return x.copyWith(workers: {...x.workers, worker: state}, unread: finished ? {...x.unread, worker} : null);
        }),
      _ when sessionEvent(message) != null => _updateStatus(s, url, (x) {
          final chat = x.chat, side = x.sideChat;
          // Not before its history: the attach after it replays what came meanwhile.
          final nextChat = chat == null || !chat.loaded ? chat : applyChatEvent(chat, message);
          final nextSide = side == null || !side.loaded ? side : applyChatEvent(side, message);
          if (identical(nextChat, chat) && identical(nextSide, side)) return x;
          return x.copyWith(chat: nextChat, sideChat: nextSide);
        }),
      _ => s,
    };

List<SessionInfo> _newestFirst(List<SessionInfo> sessions) => [...sessions]..sort((a, b) => b.updated.compareTo(a.updated));

AppState _updateStatus(AppState s, String url, ServerStatus Function(ServerStatus) update) {
  final current = s.status[url];
  if (current == null) return s; // removed meanwhile
  return s.copyWith(status: {...s.status, url: update(current)});
}
