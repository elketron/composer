// Workflows, the live view's rules and size, site.set, and the Inbox count: all pure.

import 'package:composer_app/state/actions.dart';
import 'package:composer_app/state/app_state.dart';
import 'package:composer_app/state/chat_items.dart';
import 'package:composer_app/state/chat_rows.dart';
import 'package:composer_app/state/effects.dart';
import 'package:composer_app/state/inbox.dart';
import 'package:composer_app/state/frame_size.dart';
import 'package:composer_app/state/live_view_room.dart';
import 'package:composer_app/state/live_view_size.dart';
import 'package:composer_app/state/reduce.dart';
import 'package:composer_app/state/server_entry.dart';
import 'package:composer_app/state/server_status.dart';
import 'package:composer_app/state/workflows.dart';
import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter_test/flutter_test.dart';

const home = ServerEntry(url: 'https://home.ts.net', name: 'home', token: 't1');

SiteSummary site(String name, {String? app, List<String> mcp = const []}) =>
    SiteSummary(name: name, title: name.toUpperCase(), hasJustfile: false, hasUi: false, app: app, mcp: mcp);

SessionInfo session(String id, {String? site, TurnState state = TurnState.idle, String? parent}) =>
    SessionInfo(id: id, agent: 'default', site: site, parent: parent, state: state, lastSeq: 0, updated: '2026-11-10T10:0$id');

ServerInfo info({String? viewer}) => ServerInfo(name: 'pc', version: '0.1', model: 'qwen', slots: 1, viewer: viewer);

ServerMessageApprovalRequest approval(String request, String session) =>
    ServerMessageApprovalRequest(request: request, session: session, site: 'tools', recipe: 'greet', args: const [], body: '', hash: 'h');

AppState loaded() => reduce(AppState.initial, const ServersLoaded([home])).$1;

void main() {
  group('which sites are workflows', () {
    test('an application or MCP servers make a workflow; nothing else does', () {
      expect(isWorkflow(site('3d', app: 'blender')), isTrue);
      expect(isWorkflow(site('docs', mcp: ['search'])), isTrue);
      expect(isWorkflow(site('both', app: 'blender', mcp: ['x'])), isTrue);
      expect(isWorkflow(site('plain')), isFalse);
      expect(isWorkflow(site('empty-app', app: '')), isFalse);
    });

    test('SiteSummary JSON from /api/sites: app and mcp may be absent', () {
      final with_ = SiteSummary.fromJson({'name': '3d', 'title': '3D', 'has_justfile': false, 'has_ui': false, 'app': 'blender', 'mcp': ['a', 'b']});
      final without = SiteSummary.fromJson({'name': 'p', 'title': 'P', 'has_justfile': false, 'has_ui': false});
      expect(workflowsOf([with_, without]).map((s) => s.name), ['3d']);
      expect(without.mcp, isEmpty);
      expect(without.app, isNull);
    });

    test('the status lists the workflows from the loaded sites, and counts sessions in each', () {
      var s = loaded();
      s = reduce(s, SitesLoaded(home.url, [site('3d', app: 'blender'), site('plain'), site('docs', mcp: ['x'])])).$1;
      s = reduce(s, ServerEvent(home.url, ServerMessageSessionUpdated(session: session('1', site: '3d')))).$1;
      s = reduce(s, ServerEvent(home.url, ServerMessageSessionUpdated(session: session('2', site: '3d')))).$1;
      s = reduce(s, ServerEvent(home.url, ServerMessageSessionUpdated(session: session('3')))).$1;
      s = reduce(s, ServerEvent(home.url, ServerMessageSessionUpdated(session: session('4', site: '3d', parent: '1')))).$1;
      final status = s.statusOf(home.url);
      expect(status.workflows.map((w) => w.name), ['3d', 'docs']);
      expect(status.sessionsIn('3d').map((x) => x.id).toSet(), {'1', '2'}, reason: 'workers count with their orchestrator');
      expect(status.sessionsIn('docs'), isEmpty);
    });
  });

  group('the live view is shown only for a workflow with an application on a server with a viewer', () {
    final blender = site('3d', app: 'blender');
    test('all three conditions', () {
      expect(liveViewApp(info: info(viewer: '/view'), site: blender), 'blender');
      expect(liveViewApp(info: info(), site: blender), isNull, reason: 'no viewer on this server');
      expect(liveViewApp(info: info(viewer: ''), site: blender), isNull);
      expect(liveViewApp(info: null, site: blender), isNull);
      expect(liveViewApp(info: info(viewer: '/view'), site: null), isNull, reason: 'no workflow');
      expect(liveViewApp(info: info(viewer: '/view'), site: site('docs', mcp: ['x'])), isNull, reason: 'MCP only: no application to show');
    });

    test('through the status: the session\'s site decides', () {
      var s = loaded();
      s = reduce(s, InfoReceived(home.url, info(viewer: '/view'))).$1;
      s = reduce(s, SitesLoaded(home.url, [blender, site('docs', mcp: ['x'])])).$1;
      final status = s.statusOf(home.url);
      expect(status.liveViewOf(session('1', site: '3d')), 'blender');
      expect(status.liveViewOf(session('1', site: 'docs')), isNull);
      expect(status.liveViewOf(session('1')), isNull);
      expect(status.liveViewOf(session('1', site: 'unknown')), isNull);
      expect(status.liveViewOf(null), isNull);
    });

    test('the address is the workspace URL, the viewer path and the bare page, never the token', () {
      expect(liveViewUrl('https://h.ts.net/w/default', '/view'), 'https://h.ts.net/w/default/view?bare=1');
      expect(liveViewUrl('http://10.0.2.2:7420', 'view'), 'http://10.0.2.2:7420/view?bare=1');
    });
  });

  group('site.set', () {
    test('chooses for the open chat', () {
      var s = loaded();
      s = reduce(s, SessionOpened(home.url, 'a')).$1;
      final (_, effects) = reduce(s, SiteChosen(home.url, '3d'));
      final command = (effects.single as SendCommand).command as ClientCommandSiteSet;
      expect((command.session, command.site), ('a', '3d'));
    });

    test('null leaves the workflow', () {
      var s = reduce(loaded(), SessionOpened(home.url, 'a')).$1;
      final command = (reduce(s, SiteChosen(home.url, null)).$2.single as SendCommand).command as ClientCommandSiteSet;
      expect(command.site, isNull);
      expect(command.toJson()['site'], isNull);
    });

    test('for a given session, open or not', () {
      final command = (reduce(loaded(), SiteChosen(home.url, '3d', session: 'x')).$2.single as SendCommand).command as ClientCommandSiteSet;
      expect(command.session, 'x');
    });

    test('with no chat open and no session, nothing is sent', () {
      expect(reduce(loaded(), SiteChosen(home.url, '3d')).$2, isEmpty);
    });

    test('the reply\'s sentence is a notice, an error too (the effect runner dispatches them)', () {
      var s = loaded();
      s = reduce(s, const NoticeShown('Entered 3d (3D). Started the blender application.')).$1;
      expect(s.notice?.text, contains('Entered 3d'));
      s = reduce(s, CommandFailed(home.url, 'no such site: nope')).$1;
      expect(s.notice?.text, 'no such site: nope');
    });

    test('session.updated brings the new site into the session', () {
      var s = loaded();
      s = reduce(s, ServerEvent(home.url, ServerMessageSessionUpdated(session: session('a')))).$1;
      expect(s.statusOf(home.url).session('a')!.site, isNull);
      s = reduce(s, ServerEvent(home.url, ServerMessageSessionUpdated(session: session('a', site: '3d')))).$1;
      expect(s.statusOf(home.url).session('a')!.site, '3d');
      s = reduce(s, ServerEvent(home.url, ServerMessageSessionUpdated(session: session('a')))).$1;
      expect(s.statusOf(home.url).session('a')!.site, isNull);
    });

    test('a new chat in a workflow carries the site to the effect', () {
      final create = reduce(loaded(), NewSessionRequested(home.url, project: 'game', site: '3d')).$2.single as CreateSession;
      expect((create.agent, create.project, create.site), ('default', 'game', '3d'));
      expect((reduce(loaded(), NewSessionRequested(home.url)).$2.single as CreateSession).site, isNull);
    });
  });

  group('the live view size', () {
    test('stepping and wrapping, and the page fit of each size', () {
      expect(LiveViewSize.small.next, LiveViewSize.medium);
      expect(LiveViewSize.medium.next, LiveViewSize.large);
      expect(LiveViewSize.large.next, LiveViewSize.small);
      expect([for (final s in LiveViewSize.values) s.fit], ['contain', 'contain', 'fill-height']);
    });

    LiveViewRoom room({double width = 400, FrameSize frame = FrameSize.assumed, double maxHeight = 500}) =>
        LiveViewRoom(width: width, frame: frame, maxHeight: maxHeight);

    test('small is a peek, medium shows the whole screen at the width, large takes the room', () {
      final r = room();
      expect([for (final s in LiveViewSize.values) r.heightOf(s)], [96, 250, 500]);
      final wide = room(frame: const FrameSize(1920, 1080));
      expect(wide.heightOf(LiveViewSize.medium), closeTo(225, 1e-9));
      final tall = room(frame: const FrameSize(1280, 1024));
      expect(tall.heightOf(LiveViewSize.medium), 320);
      expect(tall.heightOf(LiveViewSize.small), 96, reason: 'whatever the shape');
    });

    test('16:10 is assumed until the size is known', () {
      expect(FrameSize.assumed.aspect, 1.6);
      expect(room(width: 360).heightOf(LiveViewSize.medium), 225);
      expect(FrameSize.tryFrom(1280, 800), const FrameSize(1280, 800));
      expect(FrameSize.tryFrom(1280.0, 800), const FrameSize(1280, 800));
      expect(FrameSize.tryFrom(16384, 1), const FrameSize(16384, 1));
      for (final bad in [(0, 800), (1280, -1), (null, 800), ('1280', 800), (double.nan, 1), (double.infinity, 1), (0.5, 800), (1280, 16385), (1e300, 1e300), (double.negativeInfinity, 5)]) {
        expect(FrameSize.tryFrom(bad.$1, bad.$2), isNull, reason: '$bad');
      }
    });

    test('the chat minimum is respected: nothing is taller than the room, and never below the peek', () {
      final portrait = room(frame: const FrameSize(720, 1440), maxHeight: 400);
      expect(portrait.heightOf(LiveViewSize.medium), 400, reason: 'would be 800');
      expect(portrait.heightOf(LiveViewSize.large), 400);
      final squeezed = room(maxHeight: 40);
      expect(squeezed.heightOf(LiveViewSize.large), 96);
      expect(squeezed.heightOf(LiveViewSize.medium), 96);
      expect(squeezed.heightOf(LiveViewSize.small), 96);
      expect(squeezed.clampDrag(500), 96);
    });

    test('the heights follow a change of the frame size', () {
      expect(room().heightOf(LiveViewSize.medium), 250);
      expect(room(frame: const FrameSize(1024, 768)).heightOf(LiveViewSize.medium), 300);
      expect(room(frame: const FrameSize(800, 1280)).heightOf(LiveViewSize.medium), 500);
      final (next, effects) = reduce(AppState.initial, const LiveViewFrameSized(FrameSize(1024, 768)));
      expect(next.liveFrame, const FrameSize(1024, 768));
      expect(effects, isEmpty, reason: 'not kept on the device');
      expect(reduce(next, const LiveViewFrameSized(FrameSize(1024, 768))).$1, same(next));
      expect(reduce(next, const LiveViewFrameSized(FrameSize(1920, 1080))).$1.liveFrame, const FrameSize(1920, 1080));
      expect(AppState.initial.liveFrame, isNull);
    });

    test('a drag snaps to the nearest size', () {
      final r = room();
      expect(r.snap(0), LiveViewSize.small);
      expect(r.snap(150), LiveViewSize.small);
      expect(r.snap(180), LiveViewSize.medium);
      expect(r.snap(250), LiveViewSize.medium);
      expect(r.snap(380), LiveViewSize.large);
      expect(r.snap(900), LiveViewSize.large);
      expect(r.clampDrag(900), 500);
      expect(r.clampDrag(0), 96);
    });

    test('stepping and resizing change the state and ask to save it; the same size asks for nothing', () {
      var s = loaded();
      expect(s.liveViewSize, LiveViewSize.medium);
      var (next, effects) = reduce(s, const LiveViewStepped());
      expect(next.liveViewSize, LiveViewSize.large);
      expect((effects.single as SaveLiveViewSize).size, LiveViewSize.large);
      (next, effects) = reduce(next, const LiveViewResized(LiveViewSize.small));
      expect(next.liveViewSize, LiveViewSize.small);
      expect((effects.single as SaveLiveViewSize).size, LiveViewSize.small);
      (next, effects) = reduce(next, const LiveViewResized(LiveViewSize.small));
      expect(effects, isEmpty);
    });

    test('the saved size comes back at start; nothing saved keeps the default', () {
      expect(reduce(AppState.initial, const AppStarted()).$2.whereType<LoadPreferences>(), hasLength(1));
      expect(reduce(AppState.initial, const PreferencesLoaded(liveViewSize: LiveViewSize.large)).$1.liveViewSize, LiveViewSize.large);
      expect(reduce(AppState.initial, const PreferencesLoaded()).$1.liveViewSize, LiveViewSize.medium);
      expect(LiveViewSize.parse('large'), LiveViewSize.large);
      expect(LiveViewSize.parse('huge'), isNull);
      expect(LiveViewSize.parse(null), isNull);
    });
  });

  group('the Inbox count', () {
    test('approvals of listed sessions, unseen finished sessions and works at a gate', () {
      var s = loaded();
      s = reduce(s, ServerEvent(home.url, ServerMessageSessionUpdated(session: session('1', state: TurnState.running)))).$1;
      s = reduce(s, ServerEvent(home.url, ServerMessageSessionUpdated(session: session('2')))).$1;
      expect(Inbox.of(s.statusOf(home.url)).needsYou, 0);

      // 1 finishes while another chat is in front: it is unread.
      s = reduce(s, ServerEvent(home.url, ServerMessageSessionUpdated(session: session('1')))).$1;
      expect(Inbox.of(s.statusOf(home.url)).unread.map((x) => x.id), ['1']);
      expect(Inbox.of(s.statusOf(home.url)).needsYou, 1);

      s = reduce(s, ServerEvent(home.url, approval('r1', '2'))).$1;
      s = reduce(s, ServerEvent(home.url, approval('r2', 'gone'))).$1;
      final inbox = Inbox.of(s.statusOf(home.url));
      expect(inbox.approvals.map((a) => a.request), ['r1'], reason: 'an approval of a session that is not listed is not in the Inbox');
      expect(inbox.needsYou, 2);
    });

    test('a running session is not "unread" and the rest is recent', () {
      var s = loaded();
      s = reduce(s, ServerEvent(home.url, ServerMessageSessionUpdated(session: session('1', state: TurnState.running)))).$1;
      final inbox = Inbox.of(s.statusOf(home.url));
      expect(inbox.busy.map((x) => x.id), ['1']);
      expect(inbox.rest, isEmpty);
    });
  });

  group('the header line', () {
    test('what the session is doing, with the step of the turn', () {
      expect(stateLine('running', 6), 'working · step 6');
      expect(stateLine('running', 0), 'working');
      expect(stateLine('idle', 3), 'idle');
      expect(stateLine('approval', 0), 'needs approval');
    });

    test('steps count the tools since your last message', () {
      const user = UserItem('go');
      ToolItem tool(String id) => ToolItem(callId: id, name: 'open', summary: id);
      expect(stepsSinceLastMessage([user, tool('a'), tool('b'), const TextItem('x'), tool('c')]), 3);
      expect(stepsSinceLastMessage([user, tool('a'), user, tool('b')]), 1);
      expect(stepsSinceLastMessage(const []), 0);
    });
  });

  test('ServerStatus keeps the default of no workflows', () {
    expect(ServerStatus.initial.workflows, isEmpty);
  });
}
