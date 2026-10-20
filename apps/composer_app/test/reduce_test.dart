import 'package:composer_app/state/actions.dart';
import 'package:composer_app/state/app_state.dart';
import 'package:composer_app/state/chat_items.dart';
import 'package:composer_app/state/effects.dart';
import 'package:composer_app/state/reduce.dart';
import 'package:composer_app/state/server_entry.dart';
import 'package:composer_app/state/types.dart';
import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter_test/flutter_test.dart';

const home = ServerEntry(url: 'https://home.ts.net', name: 'home', token: 't1');
const work = ServerEntry(url: 'https://work.ts.net', name: 'work', token: 't2');
const info = ServerInfo(name: 'home-pc', version: '0.1.0', model: 'qwen', slots: 2);

SessionInfo session(String id, String updated) =>
    SessionInfo(id: id, agent: 'default', state: TurnState.idle, lastSeq: 0, updated: updated);

AppState loaded(List<ServerEntry> servers) => reduce(AppState.initial, ServersLoaded(servers, StorageKind.keyring)).$1;

void main() {
  test('loading connects every server and shows the first', () {
    final (state, effects) = reduce(AppState.initial, const ServersLoaded([home, work], StorageKind.file));
    expect(state.loaded, isTrue);
    expect(state.active, home.url);
    expect(state.storage, StorageKind.file);
    expect(effects.whereType<Connect>().map((e) => e.server.url), [home.url, work.url]);
  });

  test('pairing adds the server under the name it reports, saves, connects and selects it', () {
    var (state, effects) = reduce(loaded([work]), const PairRequested(url: 'https://home.ts.net', code: 'AB', deviceName: 'pc'));
    expect(state.pairing.busy, isTrue);
    expect(effects.single, isA<Pair>());

    (state, effects) = reduce(state, const PairSucceeded(home, info));
    expect(state.servers.map((s) => s.name), ['work', 'home-pc']);
    expect(state.active, home.url);
    expect(state.pairing.busy, isFalse);
    expect(state.statusOf(home.url).info, info);
    expect(effects.whereType<SaveServers>().single.servers.length, 2);
    expect(effects.whereType<Connect>().single.server.token, 't1');
  });

  test('pairing a known server again replaces its token', () {
    final (state, _) = reduce(loaded([home]), PairSucceeded(ServerEntry(url: home.url, name: 'x', token: 'new'), info));
    expect(state.servers.single.token, 'new');
  });

  test('a failed pairing shows why', () {
    final state = reduce(loaded([]), const PairFailed('that code didn\'t work')).$1;
    expect(state.pairing.error, contains('code'));
    expect(reduce(state, const PairingReset()).$1.pairing.error, isNull);
  });

  test('removing the active server disconnects it and selects another', () {
    final (state, effects) = reduce(loaded([home, work]), ServerRemoved(home.url));
    expect(state.servers, [work]);
    expect(state.active, work.url);
    expect(state.status.containsKey(home.url), isFalse);
    expect(effects.whereType<Disconnect>().single.url, home.url);
  });

  test('link changes and errors', () {
    var state = reduce(loaded([home]), LinkChanged(home.url, LinkState.offline, error: 'lost')).$1;
    expect(state.statusOf(home.url).error, 'lost');
    state = reduce(state, LinkChanged(home.url, LinkState.online)).$1;
    expect(state.statusOf(home.url).link, LinkState.online);
    expect(state.statusOf(home.url).error, isNull);
    // Events for a removed server are dropped.
    final removed = reduce(state, ServerRemoved(home.url)).$1;
    expect(identical(reduce(removed, LinkChanged(home.url, LinkState.online)).$1, removed), isTrue);
  });

  test('a new server name is stored', () {
    final (state, effects) = reduce(loaded([home]), InfoReceived(home.url, info));
    expect(state.servers.single.name, 'home-pc');
    expect(effects.single, isA<SaveServers>());
    expect(reduce(state, InfoReceived(home.url, info)).$2, isEmpty);
  });

  test('sessions: listed newest first, updated in place', () {
    var state = reduce(
      loaded([home]),
      ServerEvent(home.url, ServerMessageOk(id: 1, result: CommandResultSessions(sessions: [session('a', '2026-01-01'), session('b', '2026-02-01')]))),
    ).$1;
    expect(state.statusOf(home.url).sessions.map((s) => s.id), ['b', 'a']);
    state = reduce(state, ServerEvent(home.url, ServerMessageSessionUpdated(session: session('a', '2026-03-01')))).$1;
    expect(state.statusOf(home.url).sessions.map((s) => s.id), ['a', 'b']);
  });

  test('resuming retries only the offline servers', () {
    var state = loaded([home, work]);
    state = reduce(state, LinkChanged(home.url, LinkState.online)).$1;
    state = reduce(state, LinkChanged(work.url, LinkState.offline, error: 'x')).$1;
    expect(reduce(state, const ForegroundChanged(true)).$2.whereType<Reconnect>().map((e) => e.url), [work.url]);
  });

  test('switching and shortcuts only select known servers', () {
    final state = loaded([home, work]);
    expect(reduce(state, ServerSelected(work.url)).$1.active, work.url);
    expect(reduce(state, const ServerSelected('https://nope')).$1.active, home.url);
  });

  group('chat', () {
    ServerMessageApprovalRequest approval(String id) => ServerMessageApprovalRequest(
          request: id, session: 'a', site: 's', recipe: 'r', args: const [], body: 'b', hash: 'h');

    test('opening a session attaches it, switching detaches the old one, closing detaches', () {
      var (state, effects) = reduce(loaded([home]), SessionOpened(home.url, 'a'));
      expect(state.statusOf(home.url).chat!.session, 'a');
      expect(effects.cast<SendCommand>().map((e) => e.command.runtimeType), [ClientCommandSessionAttach]);

      (state, effects) = reduce(state, SessionOpened(home.url, 'b'));
      final commands = effects.cast<SendCommand>().map((e) => e.command).toList();
      expect((commands[0] as ClientCommandSessionDetach).session, 'a');
      expect((commands[1] as ClientCommandSessionAttach).session, 'b');

      (state, effects) = reduce(state, SessionClosed(home.url));
      expect(state.statusOf(home.url).chat, isNull);
      expect((effects.single as SendCommand).command, isA<ClientCommandSessionDetach>());
    });

    test('events fill the open chat; reconnecting re-attaches from the last seq', () {
      var state = reduce(loaded([home]), SessionOpened(home.url, 'a')).$1;
      state = reduce(
        state,
        ServerEvent(home.url, ServerMessageMessageDone(session: 'a', seq: 7, message: const ChatMessage(role: Role.user, content: 'hi'))),
      ).$1;
      expect(state.statusOf(home.url).chat!.items.single, isA<UserItem>());

      final (_, effects) = reduce(state, LinkChanged(home.url, LinkState.online));
      final attach = effects.whereType<SendCommand>().single.command as ClientCommandSessionAttach;
      expect(attach.since, 7);
    });

    test('unsetting the project goes to the open session', () {
      expect(reduce(loaded([home]), ProjectChosen(home.url, null)).$2, isEmpty);
      final state = reduce(loaded([home]), SessionOpened(home.url, 'a')).$1;
      final command = (reduce(state, ProjectChosen(home.url, null)).$2.single as SendCommand).command as ClientCommandProjectSet;
      expect((command.session, command.path), ('a', null));
    });

    test('attachments: upload as picked, send when ready, retry and remove', () {
      var state = reduce(loaded([home]), SessionOpened(home.url, 'a')).$1;
      final (picked, uploads) = reduce(state, FilesPicked(home.url, const [PickedFile('a.png', path: '/tmp/a.png'), PickedFile('b.md', bytes: [35])]));
      expect([for (final e in uploads) (e as UploadAttachment).id], [1, 2]);
      expect((uploads.first as UploadAttachment).session, 'a');
      state = picked;
      // Not sent while uploading.
      expect(reduce(state, MessageSubmitted(home.url, 'look')).$2, isEmpty);
      const stored = Attachment(file: 'abc-a.png', name: 'a.png', mime: 'image/png', size: 9);
      state = reduce(state, UploadProgressed(home.url, 1, 0.5)).$1;
      expect(state.status[home.url]!.chat!.uploads.first.progress, 0.5);
      state = reduce(state, UploadFinished(home.url, 1, attachment: stored)).$1;
      state = reduce(state, UploadFinished(home.url, 2, error: 'too big')).$1;
      expect(state.status[home.url]!.chat!.uploads.last.status, UploadStatus.failed);
      final retry = reduce(state, UploadRetried(home.url, 2));
      expect((retry.$2.single as UploadAttachment).id, 2);
      expect(retry.$1.status[home.url]!.chat!.uploads.last.status, UploadStatus.uploading);
      state = reduce(state, UploadRemoved(home.url, 2)).$1;
      // Ready: sent with the message (files alone are fine too), and the list empties.
      final (sent, effects) = reduce(state, MessageSubmitted(home.url, ''));
      final command = (effects.single as SendCommand).command as ClientCommandMessageSend;
      expect(command.text, '');
      expect([for (final a in command.attachments) a.file], ['abc-a.png']);
      expect(sent.status[home.url]!.chat!.uploads, isEmpty);
      // Ids keep counting, so a late answer for a removed upload can't hit a new one.
      final (_, again) = reduce(sent, FilesPicked(home.url, const [PickedFile('c.txt', path: '/tmp/c.txt')]));
      expect((again.single as UploadAttachment).id, 3);
    });

    test('external harnesses: asked, stored per project, toggled', () {
      final ask = reduce(loaded([home]), const HarnessesRequested('https://home.ts.net', '/p')).$2.single as SendCommand;
      expect((ask.command as ClientCommandHarnessList).project, '/p');
      final state = reduce(
        loaded([home]),
        ServerEvent(home.url, const ServerMessageOk(id: 1, result: CommandResultHarnesses(project: '/p', harnesses: [HarnessSwitch(entry: 'claude-code', allowed: true, on: true)]))),
      ).$1;
      expect(state.status[home.url]!.harnesses['/p']!.single.on, isTrue);
      final toggle = (reduce(state, const HarnessToggled('https://home.ts.net', '/p', 'claude-code', false)).$2.single as SendCommand).command
          as ClientCommandHarnessSet;
      expect((toggle.project, toggle.entry, toggle.on), ('/p', 'claude-code', false));
    });

    test('ticket review: changes asked, merged, discarded, failures kept on the review', () {
      const url = 'https://home.ts.net';
      final (asking, ask) = reduce(loaded([home]), const ReviewRequested(url, '0007'));
      final command = ask.single as TicketCommand;
      expect((command.ticket, (command.command as ClientCommandTicketChanges).ticket), ('0007', '0007'));
      expect(asking.status[url]!.reviews['0007']!.busy, isTrue);

      const changes = ProjectDiff(path: '/wt', files: [], diff: '');
      var state = reduce(asking, const TicketReplied(url, '0007', result: CommandResultChanges(changes: changes))).$1;
      expect((state.status[url]!.reviews['0007']!.busy, state.status[url]!.reviews['0007']!.changes), (false, changes));

      final (merging, merge) = reduce(state, const TicketMergeRequested(url, '0007'));
      expect((merge.single as TicketCommand).command, isA<ClientCommandTicketMerge>());
      state = reduce(merging, const TicketReplied(url, '0007', error: 'conflicts with main in: a.rs')).$1;
      expect(state.status[url]!.reviews['0007']!.error, contains('conflicts'));
      expect(state.status[url]!.reviews['0007']!.changes, changes, reason: 'still shown');
      state = reduce(state, const TicketReplied(url, '0007', result: CommandResultMerged(commit: 'abc1234'))).$1;
      expect((state.status[url]!.reviews['0007']!.merged, state.status[url]!.reviews['0007']!.error), ('abc1234', null));
      expect(state.notice!.text, 'Ticket 0007 merged as abc1234');

      final discard = reduce(state, const TicketDiscardRequested(url, '0008')).$2.single as TicketCommand;
      expect(discard.command, isA<ClientCommandTicketDiscard>());
      state = reduce(state, const TicketReplied(url, '0008', result: CommandResultDone())).$1;
      expect(state.status[url]!.reviews['0008']!.discarded, isTrue);
    });

    test('messages and cancel go to the open session only', () {
      expect(reduce(loaded([home]), MessageSubmitted(home.url, 'x')).$2, isEmpty);
      final state = reduce(loaded([home]), SessionOpened(home.url, 'a')).$1;
      final send = (reduce(state, MessageSubmitted(home.url, 'x')).$2.single as SendCommand).command as ClientCommandMessageSend;
      expect((send.session, send.text), ('a', 'x'));
      expect((reduce(state, CancelRequested(home.url)).$2.single as SendCommand).command, isA<ClientCommandTurnCancel>());
    });

    test('approvals: collected once, removed when answered or resolved, refreshed on reconnect', () {
      var state = loaded([home]);
      state = reduce(state, ServerEvent(home.url, approval('r1'))).$1;
      state = reduce(state, ServerEvent(home.url, approval('r1'))).$1;
      state = reduce(state, ServerEvent(home.url, approval('r2'))).$1;
      expect(state.statusOf(home.url).approvals.map((a) => a.request), ['r1', 'r2']);

      final (answered, effects) = reduce(state, ApprovalAnswered(home.url, 'r1', ApprovalDecision.approve));
      expect(answered.statusOf(home.url).approvals.map((a) => a.request), ['r2']);
      final respond = effects.whereType<SendCommand>().single.command as ClientCommandApprovalRespond;
      expect(respond.decision, ApprovalDecision.approve);

      state = reduce(answered, ServerEvent(home.url, const ServerMessageApprovalResolved(request: 'r2', session: 'a', approved: false))).$1;
      expect(state.statusOf(home.url).approvals, isEmpty);

      state = reduce(state, ServerEvent(home.url, approval('r3'))).$1;
      expect(reduce(state, LinkChanged(home.url, LinkState.online)).$1.statusOf(home.url).approvals, isEmpty);
    });

    test('failed commands become notices', () {
      final state = reduce(loaded([home]), CommandFailed(home.url, 'no such session')).$1;
      expect(state.notice!.text, 'no such session');
      expect(reduce(state, const NoticeShown('x')).$1.notice!.id, state.notice!.id + 1);
    });
  });

  group('reader', () {
    PageResponse page(String url) =>
        PageResponse(url: url, title: url, frontmatter: const {}, raw: '# x', backlinks: const [], hash: 'h');

    AppState opened(String url) {
      var state = reduce(loaded([home]), PageRequested(home.url, url)).$1;
      return reduce(state, PageLoaded(home.url, url, page(url))).$1;
    }

    test('navigating fetches, keeps history, back and forward', () {
      var (state, effects) = reduce(loaded([home]), PageRequested(home.url, 'site://memory'));
      expect(state.statusOf(home.url).page.loading, 'site://memory');
      expect((effects.single as FetchPage).page, 'site://memory');
      state = reduce(state, PageLoaded(home.url, 'site://memory', page('site://memory/index.md'))).$1;
      state = reduce(state, PageRequested(home.url, 'site://memory/user.md')).$1;
      state = reduce(state, PageLoaded(home.url, 'site://memory/user.md', page('site://memory/user.md'))).$1;
      expect(state.statusOf(home.url).page.back, ['site://memory/index.md']);

      (state, effects) = reduce(state, PageBack(home.url));
      expect((effects.single as FetchPage).page, 'site://memory/index.md');
      expect(state.statusOf(home.url).page.forward, ['site://memory/user.md']);
      expect(state.statusOf(home.url).page.back, isEmpty);
    });

    test('a late answer for an abandoned request is dropped; failures show', () {
      var state = reduce(loaded([home]), PageRequested(home.url, 'site://a')).$1;
      state = reduce(state, PageRequested(home.url, 'site://b')).$1;
      state = reduce(state, PageLoaded(home.url, 'site://a', page('site://a/index.md'))).$1;
      expect(state.statusOf(home.url).page.page, isNull);
      state = reduce(state, PageFailed(home.url, 'site://b', 'no such page')).$1;
      expect(state.statusOf(home.url).page.error, 'site://b: no such page');
      expect(state.statusOf(home.url).page.loading, isNull);
    });

    test('the page on screen reloads when it changes on disk', () {
      final state = opened('site://memory/user.md');
      final changed = ServerEvent(
        home.url,
        const ServerMessagePageChanged(url: 'site://memory/user.md', kind: PageChangeKind.modified, by: ChangedBy.agent),
      );
      expect((reduce(state, changed).$2.single as FetchPage).page, 'site://memory/user.md');
      final other = ServerEvent(
        home.url,
        const ServerMessagePageChanged(url: 'site://memory/x.md', kind: PageChangeKind.modified, by: ChangedBy.agent),
      );
      expect(reduce(state, other).$2, isEmpty);
    });

    test('follows the agent in the open session, live events only, unless turned off', () {
      final sessions = ServerEvent(
        home.url,
        ServerMessageOk(id: 1, result: CommandResultSessions(sessions: [SessionInfo(id: 'a', agent: 'default', state: TurnState.idle, lastSeq: 5, updated: '')])),
      );
      var state = reduce(loaded([home]), sessions).$1;
      state = reduce(state, SessionOpened(home.url, 'a')).$1;
      ServerEvent nav(int seq, String session) =>
          ServerEvent(home.url, ServerMessageAgentNavigated(session: session, seq: seq, url: 'site://plans/x'));

      expect(reduce(state, nav(4, 'a')).$2, isEmpty, reason: 'a replay');
      expect(reduce(state, nav(6, 'b')).$2, isEmpty, reason: 'another session');
      final (followed, effects) = reduce(state, nav(6, 'a'));
      expect((effects.single as FetchPage).page, 'site://plans/x');
      expect(followed.statusOf(home.url).page.loading, 'site://plans/x');

      state = reduce(state, FollowToggled(home.url)).$1;
      expect(reduce(state, nav(7, 'a')).$2, isEmpty);
    });

    test('apps show in the reader; closing keeps the follow choice', () {
      var state = reduce(opened('site://orchestration'), AppRequested(home.url, 'orchestration')).$1;
      expect(state.statusOf(home.url).page.appSite, 'orchestration');
      state = reduce(state, FollowToggled(home.url)).$1;
      state = reduce(state, ReaderClosed(home.url)).$1;
      expect(state.statusOf(home.url).page.isOpen, isFalse);
      expect(state.statusOf(home.url).page.follow, isFalse);
    });

    test('coming online fetches the site list', () {
      final (_, effects) = reduce(loaded([home]), LinkChanged(home.url, LinkState.online));
      expect(effects.whereType<FetchCatalog>().single.url, home.url);
    });
  });

  group('notifications', () {
    ServerMessageApprovalRequest approval(String id) => ServerMessageApprovalRequest(
          request: id, session: 'a', site: 'tools', recipe: 'greet', args: const ['bob'], body: 'b', hash: 'h');
    SessionInfo info(String id, TurnState state, {String? ticket}) =>
        SessionInfo(id: id, agent: 'default', state: state, lastSeq: 1, updated: '', ticket: ticket, preview: 'hi');
    AppState withSessions(AppState s, List<SessionInfo> sessions) =>
        reduce(s, ServerEvent(home.url, ServerMessageOk(id: 1, result: CommandResultSessions(sessions: sessions)))).$1;
    AppState background(AppState s) => reduce(s, const ForegroundChanged(false)).$1;

    test('approvals: only in the background, once, withdrawn when answered or resolved', () {
      var state = loaded([home]);
      expect(reduce(state, ServerEvent(home.url, approval('r1'))).$2.whereType<Notify>(), isEmpty, reason: 'the bar shows it');

      state = background(state);
      var (next, effects) = reduce(state, ServerEvent(home.url, approval('r2')));
      final note = effects.whereType<Notify>().single;
      expect((note.title, note.approval, note.session), ('Run recipe? tools › greet bob', 'r2', 'a'));
      // A reconnect re-sends it: no second notification.
      next = reduce(next, LinkChanged(home.url, LinkState.online)).$1;
      expect(reduce(next, ServerEvent(home.url, approval('r2'))).$2.whereType<Notify>(), isEmpty);

      expect(
        reduce(next, ServerEvent(home.url, const ServerMessageApprovalResolved(request: 'r2', session: 'a', approved: true))).$2,
        [isA<CancelNotification>().having((c) => c.key, 'key', 'approval:r2')],
      );
      expect(reduce(next, ApprovalAnswered(home.url, 'r2', ApprovalDecision.deny)).$2.whereType<CancelNotification>(), hasLength(1));
    });

    test('reminders: always notified, leading to the page or the session', () {
      const due = ServerMessageReminderDue(
          url: 'site://calendar/reminders/call.md', title: 'Call', due: '2026-10-19 20:00', late: false);
      final note = reduce(loaded([home]), ServerEvent(home.url, due)).$2.whereType<Notify>().single;
      expect((note.title, note.body, note.page, note.session), ('Call', 'due 2026-10-19 20:00 · home', 'site://calendar/reminders/call.md', null));
      const late = ServerMessageReminderDue(
          url: 'site://calendar/reminders/r.md', title: 'Review', due: '2026-10-19 08:00', late: true, session: 'ab12');
      final agent = reduce(background(loaded([home])), ServerEvent(home.url, late)).$2.whereType<Notify>().single;
      expect((agent.body, agent.session), ('was due 2026-10-19 08:00 · home', 'ab12'));
    });

    test('going to the background notifies approvals still open', () {
      var state = reduce(loaded([home]), ServerEvent(home.url, approval('r1'))).$1;
      final (next, effects) = reduce(state, const ForegroundChanged(false));
      expect(effects.whereType<Notify>().single.approval, 'r1');
      state = reduce(reduce(next, const ForegroundChanged(true)).$1, const ForegroundChanged(false)).$1;
      expect(reduce(state, const ForegroundChanged(false)).$2.whereType<Notify>(), isEmpty);
    });

    test('a finished turn: notified unless you are looking at it; never for list reloads', () {
      var state = withSessions(loaded([home]), [info('a', TurnState.running)]);
      final done = ServerEvent(home.url, ServerMessageSessionUpdated(session: info('a', TurnState.idle)));
      final (_, effects) = reduce(state, done);
      expect(effects.whereType<Notify>().single.session, 'a');

      final watching = reduce(state, SessionOpened(home.url, 'a')).$1;
      expect(reduce(watching, done).$2.whereType<Notify>(), isEmpty);
      expect(reduce(background(watching), done).$2.whereType<Notify>(), hasLength(1));

      // Idle to idle, or the list arriving after a reconnect: nothing.
      state = withSessions(loaded([home]), [info('a', TurnState.idle)]);
      expect(reduce(state, done).$2.whereType<Notify>(), isEmpty);
      // Workers are reported by worker.status instead.
      state = withSessions(loaded([home]), [info('w', TurnState.running, ticket: '0001')]);
      expect(
        reduce(state, ServerEvent(home.url, ServerMessageSessionUpdated(session: info('w', TurnState.idle, ticket: '0001')))).$2,
        isEmpty,
      );
    });

    test('a finished worker leads to its orchestrator', () {
      final status = ServerEvent(
        home.url,
        const ServerMessageWorkerStatus(worker: 'w', ticket: '0001', parent: 'o', state: WorkerState.done),
      );
      final note = reduce(loaded([home]), status).$2.whereType<Notify>().single;
      expect((note.title, note.session), ('Worker finished ticket 0001', 'o'));
      final running = ServerEvent(
        home.url,
        const ServerMessageWorkerStatus(worker: 'w', ticket: '0001', parent: 'o', state: WorkerState.running),
      );
      expect(reduce(loaded([home]), running).$2, isEmpty);
    });

    test('the worker you watch finishing hands back to its orchestrator, unless you are writing', () {
      SessionInfo member(String id, {String? parent, TurnState state = TurnState.idle}) =>
          SessionInfo(id: id, agent: 'default', state: state, lastSeq: 0, updated: '', parent: parent, ticket: parent == null ? null : '0001');
      final sessions = [member('x'), member('o'), member('w1', parent: 'o'), member('w2', parent: 'o', state: TurnState.running)];
      var state = reduce(withSessions(loaded([home]), sessions), SessionOpened(home.url, 'w1')).$1;
      final status = state.statusOf(home.url);
      expect(status.orchestration('w1').map((s) => s.id), ['o', 'w1', 'w2']);
      expect(status.orchestration('o').map((s) => s.id), ['o', 'w1', 'w2']);
      expect(status.orchestration('x'), isEmpty);
      expect(status.activity(status.session('w2')!), 'running');

      ServerEvent finished(String worker, WorkerState state) =>
          ServerEvent(home.url, ServerMessageWorkerStatus(worker: worker, ticket: '0001', parent: 'o', state: state));
      // Another worker: you stay, and it shows as done.
      var (next, effects) = reduce(state, finished('w2', WorkerState.done));
      expect(next.statusOf(home.url).chat!.session, 'w1');
      expect(next.statusOf(home.url).activity(member('w2', parent: 'o')), 'done');

      (next, effects) = reduce(next, finished('w1', WorkerState.done));
      expect(next.statusOf(home.url).chat!.session, 'o');
      expect((effects.whereType<SendCommand>().last.command as ClientCommandSessionAttach).session, 'o');
      expect(next.notice!.text, contains('back to the orchestrator'));
      expect(effects.whereType<Notify>(), isEmpty, reason: 'you are looking at the orchestrator now');

      state = reduce(state, DraftChanged(home.url, drafting: true)).$1;
      expect(reduce(state, finished('w1', WorkerState.failed)).$1.statusOf(home.url).chat!.session, 'w1');
    });

    test('the phone home: unread work, recent pages, search, new chats with a project', () {
      SessionInfo s(String id, TurnState state) => SessionInfo(id: id, agent: 'default', state: state, lastSeq: 0, updated: '');
      var state = withSessions(loaded([home]), [s('a', TurnState.running), s('b', TurnState.running)]);
      state = reduce(state, SessionOpened(home.url, 'b')).$1;
      ServerEvent idle(String id) => ServerEvent(home.url, ServerMessageSessionUpdated(session: s(id, TurnState.idle)));
      state = reduce(reduce(state, idle('a')).$1, idle('b')).$1;
      expect(state.statusOf(home.url).unread, {'a'}, reason: 'b was on screen');
      state = reduce(state, SessionOpened(home.url, 'a')).$1;
      expect(state.statusOf(home.url).unread, isEmpty, reason: 'opening it reads it');

      ServerEvent changed(String url, ChangedBy by) =>
          ServerEvent(home.url, ServerMessagePageChanged(url: url, kind: PageChangeKind.modified, by: by));
      for (final e in [changed('site://a.md', ChangedBy.agent), changed('site://b.md', ChangedBy.external), changed('site://a.md', ChangedBy.app)]) {
        state = reduce(state, e).$1;
      }
      expect(state.statusOf(home.url).recent.map((r) => (r.url, r.by)), [('site://a.md', ChangedBy.app)], reason: 'not your own edits; no repeats');

      var (next, effects) = reduce(state, SearchChanged(home.url, 'type:ticket'));
      expect(effects.whereType<SearchPages>().single.query, 'type:ticket');
      next = reduce(next, SearchChanged(home.url, 'type:plan')).$1;
      const hit = SearchHit(url: 'site://x.md', title: 'X', site: 'x');
      expect(reduce(next, SearchLoaded(home.url, 'type:ticket', const [hit])).$1.statusOf(home.url).search.hits, isNull, reason: 'stale');
      expect(reduce(next, SearchLoaded(home.url, 'type:plan', const [hit])).$1.statusOf(home.url).search.hits, [hit]);

      final create = reduce(state, NewSessionRequested(home.url, agent: 'orchestrator', project: 'composer')).$2.single as CreateSession;
      expect((create.agent, create.project), ('orchestrator', 'composer'));
      expect(reduce(state, const TabSelected(PhoneTab.pages)).$1.tab, PhoneTab.pages);
    });

    test('Android keeps running while there are servers', () {
      expect(reduce(AppState.initial, const ServersLoaded([home, work], StorageKind.keyring)).$2.whereType<KeepAlive>().single.servers, 2);
      expect(reduce(loaded([home]), ServerRemoved(home.url)).$2.whereType<KeepAlive>().single.servers, 0);
    });
  });
}
