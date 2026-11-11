// The session screen's workflow chip and sheet, the live view's handle, the bottom bar, the
// Workflows tab and the chat order, with the real reducer and widgets.

import 'package:composer_app/flux/store.dart';
import 'package:composer_app/state/actions.dart';
import 'package:composer_app/state/app_state.dart';
import 'package:composer_app/state/effects.dart';
import 'package:composer_app/state/frame_size.dart';
import 'package:composer_app/state/live_view_size.dart';
import 'package:composer_app/state/reduce.dart';
import 'package:composer_app/state/server_entry.dart';
import 'package:composer_app/state/types.dart';
import 'package:composer_app/ui/chat/chat_page.dart';
import 'package:composer_app/ui/chat/chat_view.dart';
import 'package:composer_app/ui/chat/live_view_request.dart';
import 'package:composer_app/ui/chat/page_cover.dart';
import 'package:composer_app/ui/home/phone_tabs.dart';
import 'package:composer_app/ui/home/workflows_tab.dart';
import 'package:composer_app/ui/store_scope.dart';
import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

const home = ServerEntry(url: 'https://home.ts.net', name: 'home', token: 't1');

SiteSummary site(String name, {String? app, List<String> mcp = const []}) =>
    SiteSummary(name: name, title: 'The $name', hasJustfile: false, hasUi: false, app: app, mcp: mcp);

SessionInfo session(String id, {String? site, TurnState state = TurnState.idle}) =>
    SessionInfo(id: id, agent: 'default', site: site, state: state, lastSeq: 0, updated: '2026-11-10T10:00:0$id');

ServerMessage said(int seq, String text) =>
    ServerMessageMessageDone(session: 'a', seq: seq, message: ChatMessage(role: Role.user, content: text));

void main() {
  late AppStore store;
  late List<AppEffect> effects;

  /// A store with the server loaded, session `a` open (in [inSite]), and the catalog in.
  void setUp({String? inSite, String? viewer = '/view', bool foreground = true, TurnState state = TurnState.idle}) {
    effects = [];
    store = Store(AppState.initial, reduce, (effect, dispatch) => effects.add(effect));
    store.dispatch(const ServersLoaded([home]));
    store.dispatch(InfoReceived(home.url, ServerInfo(name: 'pc', version: '0.1', model: 'qwen3 · local', slots: 1, viewer: viewer)));
    store.dispatch(SitesLoaded(home.url, [site('3d', app: 'blender'), site('docs', mcp: ['search']), site('plain')]));
    store.dispatch(ServerEvent(home.url, ServerMessageSessionUpdated(session: session('a', site: inSite, state: state))));
    store.dispatch(SessionOpened(home.url, 'a'));
    store.dispatch(HistoryLoaded(home.url, 'a', null, [said(1, 'older'), said(2, 'newer')]));
    store.dispatch(ForegroundChanged(foreground));
    effects.clear();
  }

  Future<void> pumpPage(WidgetTester tester) async {
    await tester.binding.setSurfaceSize(const Size(400, 900));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(StoreScope(
      store: store,
      child: MaterialApp(
        home: ChatPage(url: home.url, liveViewBuilder: (_, r) => Text('stub ${r.app} ${r.address} ${r.workspace} ${r.token}', key: const Key('stub'))),
      ),
    ));
    await tester.pump();
  }

  group('the workflow chip', () {
    testWidgets('names the workflow, or says there is none', (tester) async {
      setUp();
      await pumpPage(tester);
      expect(find.descendant(of: find.byKey(const Key('workflow-chip')), matching: find.text('no workflow')), findsOneWidget);

      store.dispatch(ServerEvent(home.url, ServerMessageSessionUpdated(session: session('a', site: '3d'))));
      await tester.pump();
      expect(find.descendant(of: find.byKey(const Key('workflow-chip')), matching: find.text('3d')), findsOneWidget);
      expect(find.text('qwen3 · local'), findsOneWidget, reason: 'the model is at the right');
    });

    testWidgets('the chip is a 44px target', (tester) async {
      setUp();
      await pumpPage(tester);
      expect(tester.getSize(find.byKey(const Key('workflow-chip'))).height, greaterThanOrEqualTo(44));
    });

    testWidgets('the sheet lists the workflows (not plain sites), marks the current one and sends site.set', (tester) async {
      setUp(inSite: '3d');
      await pumpPage(tester);
      await tester.tap(find.byKey(const Key('workflow-chip')));
      await tester.pumpAndSettle();
      expect(effects.whereType<FetchCatalog>(), hasLength(1), reason: 'the sheet asks for the sites again');
      expect(find.byKey(const Key('workflow-pick-3d')), findsOneWidget);
      expect(find.byKey(const Key('workflow-pick-docs')), findsOneWidget);
      expect(find.byKey(const Key('workflow-pick-plain')), findsNothing);
      expect(find.descendant(of: find.byKey(const Key('workflow-pick-3d')), matching: find.byIcon(Icons.check)), findsOneWidget);
      expect(find.descendant(of: find.byKey(const Key('workflow-pick-docs')), matching: find.byIcon(Icons.check)), findsNothing);

      await tester.tap(find.byKey(const Key('workflow-pick-docs')));
      await tester.pumpAndSettle();
      final command = effects.whereType<SendCommand>().map((e) => e.command).whereType<ClientCommandSiteSet>().single;
      expect((command.session, command.site), ('a', 'docs'));
      expect(find.byKey(const Key('workflow-pick-docs')), findsNothing, reason: 'the sheet closed');
    });

    testWidgets('leave workflow sends site null; without a workflow there is nothing to leave', (tester) async {
      setUp(inSite: '3d');
      await pumpPage(tester);
      await tester.tap(find.byKey(const Key('workflow-chip')));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const Key('workflow-leave')));
      await tester.pumpAndSettle();
      final command = effects.whereType<SendCommand>().map((e) => e.command).whereType<ClientCommandSiteSet>().single;
      expect(command.site, isNull);

      setUp();
      await pumpPage(tester);
      await tester.tap(find.byKey(const Key('workflow-chip')));
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('workflow-leave')), findsNothing);
    });

    testWidgets('choosing the current workflow sends nothing', (tester) async {
      setUp(inSite: '3d');
      await pumpPage(tester);
      await tester.tap(find.byKey(const Key('workflow-chip')));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const Key('workflow-pick-3d')));
      await tester.pumpAndSettle();
      expect(effects.whereType<SendCommand>(), isEmpty);
    });
  });

  group('the live view', () {
    testWidgets('appears for a workflow with an application on a server with a viewer; the token asks for its session, never in its URL', (tester) async {
      setUp(inSite: '3d');
      await pumpPage(tester);
      expect(find.byKey(const Key('live-view')), findsOneWidget);
      final stub = tester.widget<Text>(find.byKey(const Key('stub'))).data!;
      expect(stub, 'stub blender https://home.ts.net/view?bare=1 https://home.ts.net t1');
    });

    testWidgets('is never created otherwise', (tester) async {
      for (final (label, inSite, viewer) in [
        ('no workflow', null, '/view'),
        ('MCP only', 'docs', '/view'),
        ('a plain site', 'plain', '/view'),
        ('no viewer on the server', '3d', null),
      ]) {
        setUp(inSite: inSite, viewer: viewer);
        await pumpPage(tester);
        expect(find.byKey(const Key('live-view')), findsNothing, reason: label);
        expect(find.byKey(const Key('stub')), findsNothing, reason: label);
        expect(find.byKey(const Key('live-view-handle')), findsNothing, reason: label);
      }
    });

    testWidgets('comes and goes with the session\'s site', (tester) async {
      setUp();
      await pumpPage(tester);
      expect(find.byKey(const Key('live-view')), findsNothing);
      store.dispatch(ServerEvent(home.url, ServerMessageSessionUpdated(session: session('a', site: '3d'))));
      await tester.pump();
      expect(find.byKey(const Key('live-view')), findsOneWidget);
      store.dispatch(ServerEvent(home.url, ServerMessageSessionUpdated(session: session('a'))));
      await tester.pump();
      expect(find.byKey(const Key('live-view')), findsNothing);
      expect(find.byKey(const Key('stub')), findsNothing);
    });

    testWidgets('is not kept while the app is in the background', (tester) async {
      setUp(inSite: '3d');
      await pumpPage(tester);
      expect(find.byKey(const Key('stub')), findsOneWidget);
      store.dispatch(const ForegroundChanged(false));
      await tester.pump();
      expect(find.byKey(const Key('stub')), findsNothing, reason: 'no webview in the background');
      expect(find.byKey(const Key('live-view-paused')), findsOneWidget);
      expect(tester.getSize(find.byKey(const Key('live-view'))).height, 250, reason: 'the layout does not jump');
      store.dispatch(const ForegroundChanged(true));
      await tester.pump();
      expect(find.byKey(const Key('stub')), findsOneWidget);
    });

    testWidgets('is not kept while another page covers the screen', (tester) async {
      setUp(inSite: '3d');
      effects = [];
      await tester.binding.setSurfaceSize(const Size(400, 900));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final cover = PageCover();
      await tester.pumpWidget(StoreScope(
        store: store,
        child: PageCoverScope(
          cover: cover,
          child: MaterialApp(
            navigatorObservers: [cover],
            home: ChatPage(url: home.url, liveViewBuilder: (_, r) => Text('stub ${r.app}', key: const Key('stub'))),
          ),
        ),
      ));
      await tester.pump();
      expect(find.byKey(const Key('stub')), findsOneWidget);

      final navigator = tester.state<NavigatorState>(find.byType(Navigator));
      navigator.push(MaterialPageRoute<void>(builder: (_) => const Scaffold(body: Text('reader'))));
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('stub'), skipOffstage: false), findsNothing, reason: 'the page on top is not the chat');
      navigator.pop();
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('stub')), findsOneWidget);

      // A sheet or a menu over it is not a page: the view stays.
      showModalBottomSheet<void>(context: tester.element(find.byType(ChatPage)), builder: (_) => const Text('a sheet'));
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('stub'), skipOffstage: false), findsOneWidget);
    });

    testWidgets('a tap on the handle steps to the next size and the size is saved', (tester) async {
      setUp(inSite: '3d');
      await pumpPage(tester);
      double height() => tester.getSize(find.byKey(const Key('live-view'))).height;
      expect(height(), 250, reason: 'medium: 400 wide at the assumed 16:10');
      expect(tester.getSemantics(find.byKey(const Key('live-view-handle'))).value, 'medium');

      await tester.tap(find.byKey(const Key('live-view-handle')));
      await tester.pump();
      final large = height();
      expect(large, greaterThan(400), reason: 'as tall as the layout allows');
      expect(tester.getSize(find.byType(ChatView)).height, greaterThanOrEqualTo(220), reason: 'the chat keeps its minimum');
      expect(store.state.liveViewSize, LiveViewSize.large);
      expect(effects.whereType<SaveLiveViewSize>().last.size, LiveViewSize.large);

      await tester.tap(find.byKey(const Key('live-view-handle')));
      await tester.pump();
      expect(height(), 96, reason: 'large wraps to small');
      expect(tester.getSemantics(find.byKey(const Key('live-view-handle'))).value, 'small');
      await tester.tap(find.byKey(const Key('live-view-handle')));
      await tester.pump();
      expect(height(), 250);
    });

    testWidgets('the heights follow the screen\'s shape: 16:10 until it is known, then what the page reports', (tester) async {
      setUp(inSite: '3d');
      LiveViewRequest? request;
      await tester.binding.setSurfaceSize(const Size(400, 900));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(StoreScope(
        store: store,
        child: MaterialApp(home: ChatPage(url: home.url, liveViewBuilder: (_, r) => (request = r).app.isEmpty ? const SizedBox() : const SizedBox(key: Key('stub')))),
      ));
      await tester.pump();
      double height() => tester.getSize(find.byKey(const Key('live-view'))).height;
      expect(store.state.liveFrame, isNull);
      expect(height(), 250);
      expect(request!.size, LiveViewSize.medium);

      request!.onFrameSize(const FrameSize(1280, 1024));
      await tester.pump();
      expect(store.state.liveFrame, const FrameSize(1280, 1024));
      expect(height(), 320, reason: '400 / 1.25: the whole screen at the width, no bands');

      request!.onFrameSize(const FrameSize(1920, 1080));
      await tester.pump();
      expect(height(), closeTo(225, 0.01), reason: 'the resolution changed: the height follows');

      // A portrait screen would be taller than the layout allows: capped, the chat keeps its room.
      request!.onFrameSize(const FrameSize(720, 1440));
      await tester.pump();
      expect(tester.getSize(find.byType(ChatView)).height, greaterThanOrEqualTo(220));
      expect(height(), lessThan(800));

      // Small stays a peek whatever the shape; large is as tall as medium can get.
      request!.onFrameSize(const FrameSize(1600, 1000));
      store.dispatch(const LiveViewResized(LiveViewSize.small));
      await tester.pump();
      expect(height(), 96);
      expect(request!.size.fit, 'contain');
      store.dispatch(const LiveViewResized(LiveViewSize.large));
      await tester.pump();
      expect(request!.size.fit, 'fill-height');
      expect(store.state.liveViewSize, LiveViewSize.large, reason: 'the preference is the name, not a height');
    });

    testWidgets('the handle row holds the name, the grip and "view only"; nothing is over the picture', (tester) async {
      setUp(inSite: '3d');
      await pumpPage(tester);
      final handle = find.byKey(const Key('live-view-handle'));
      expect(tester.getSize(handle).height, 44);
      final name = find.byKey(const Key('live-view-app'));
      final only = find.byKey(const Key('live-view-only'));
      final grip = find.byKey(const Key('live-view-grip'));
      for (final f in [name, only, grip, find.byKey(const Key('live-view-dot'))]) {
        expect(find.descendant(of: handle, matching: f), findsOneWidget);
      }
      expect(find.text('blender · live'), findsOneWidget);
      expect(find.text('view only'), findsOneWidget);
      expect(find.textContaining('drag or tap'), findsNothing);
      // Left, middle, right.
      expect(tester.getCenter(name).dx, lessThan(tester.getCenter(grip).dx));
      expect(tester.getCenter(grip).dx, closeTo(200, 1));
      expect(tester.getCenter(only).dx, greaterThan(tester.getCenter(grip).dx));
      expect(tester.getTopLeft(name).dx, lessThan(60));
      expect(tester.getTopRight(only).dx, greaterThan(340));
      // Under the picture, and nothing but the picture inside its box.
      final view = find.byKey(const Key('live-view'));
      expect(tester.getTopLeft(handle).dy, tester.getBottomLeft(view).dy);
      expect(find.descendant(of: view, matching: find.byType(Text)), findsOneWidget, reason: 'the stub only');
      expect(find.descendant(of: view, matching: find.byKey(const Key('live-view-app'))), findsNothing);
      expect(find.descendant(of: view, matching: find.text('view only')), findsNothing);
      // The size stays reachable for screen readers.
      final semantics = tester.getSemantics(handle);
      expect(semantics.label, 'Resize the live view');
      expect(semantics.value, 'medium');
      expect(semantics.increasedValue, 'large');
      expect(semantics.decreasedValue, 'small');
    });

    testWidgets('the dot goes out when the page cannot load, and comes back', (tester) async {
      setUp(inSite: '3d');
      LiveViewRequest? request;
      await tester.binding.setSurfaceSize(const Size(400, 900));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(StoreScope(
        store: store,
        child: MaterialApp(home: ChatPage(url: home.url, liveViewBuilder: (_, r) => (request = r).app.isEmpty ? const SizedBox() : const SizedBox())),
      ));
      await tester.pump();
      Color? dot() => ((tester.widget<Container>(find.byKey(const Key('live-view-dot')))).decoration! as BoxDecoration).color;
      final live = dot();
      request!.onLive(false);
      await tester.pump();
      expect(dot(), isNot(live));
      request!.onLive(true);
      await tester.pump();
      expect(dot(), live);
    });

    testWidgets('dragging the handle resizes live and snaps to the nearest size on release', (tester) async {
      setUp(inSite: '3d');
      await pumpPage(tester);
      double height() => tester.getSize(find.byKey(const Key('live-view'))).height;
      final handle = find.byKey(const Key('live-view-handle'));

      final down = await tester.startGesture(tester.getCenter(handle));
      for (var i = 1; i <= 10; i++) {
        await down.moveBy(const Offset(0, 25), timeStamp: Duration(milliseconds: 16 * i));
        await tester.pump(const Duration(milliseconds: 16));
      }
      expect(height(), greaterThan(250), reason: 'follows the finger');
      expect(store.state.liveViewSize, LiveViewSize.medium, reason: 'not committed before release');
      await down.up();
      await tester.pump();
      expect(store.state.liveViewSize, LiveViewSize.large, reason: 'about 460: nearer large (about 530) than medium (250)');

      final up = await tester.startGesture(tester.getCenter(handle));
      for (var i = 1; i <= 20; i++) {
        await up.moveBy(const Offset(0, -40), timeStamp: Duration(milliseconds: 16 * i));
        await tester.pump(const Duration(milliseconds: 16));
      }
      expect(height(), 96, reason: 'never below small');
      await up.up();
      await tester.pump();
      expect(store.state.liveViewSize, LiveViewSize.small);
      expect(effects.whereType<SaveLiveViewSize>().map((e) => e.size), [LiveViewSize.large, LiveViewSize.small]);
    });

    testWidgets('a short screen caps the view so the chat keeps room', (tester) async {
      setUp(inSite: '3d');
      store.dispatch(const LiveViewResized(LiveViewSize.large));
      await tester.binding.setSurfaceSize(const Size(400, 600));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(StoreScope(
        store: store,
        child: MaterialApp(home: ChatPage(url: home.url, liveViewBuilder: (_, _) => const SizedBox())),
      ));
      await tester.pump();
      expect(tester.getSize(find.byKey(const Key('live-view'))).height, lessThan(380));
      expect(tester.getSize(find.byType(ChatView)).height, greaterThanOrEqualTo(220));
      expect(tester.takeException(), isNull);
    });
  });

  group('the header', () {
    testWidgets('shows the stop control only while a turn runs, 44px, with a label', (tester) async {
      setUp();
      await pumpPage(tester);
      expect(find.byKey(const Key('stop')), findsNothing);
      expect(find.text('idle · #a'), findsOneWidget);

      store.dispatch(ServerEvent(home.url, ServerMessageSessionUpdated(session: session('a', state: TurnState.running))));
      await tester.pump();
      expect(tester.getSize(find.byKey(const Key('stop'))), const Size(44, 44));
      expect(find.byTooltip('Stop the agent'), findsOneWidget);
      expect(find.text('working · #a'), findsOneWidget);
      await tester.tap(find.byKey(const Key('stop')));
      expect(effects.whereType<SendCommand>().map((e) => e.command).whereType<ClientCommandTurnCancel>(), hasLength(1));
    });

    testWidgets('the menu still holds the desk and pages besides the old entries', (tester) async {
      setUp();
      await pumpPage(tester);
      await tester.tap(find.byKey(const Key('chat-menu')));
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('desk')), findsOneWidget);
      expect(find.byKey(const Key('pages')), findsOneWidget);
      expect(find.text('Set project…'), findsOneWidget);
      expect(find.text('Compact (save memory, summarize)'), findsOneWidget);
    });
  });

  group('the bottom bar', () {
    testWidgets('inbox · workflows · pages · server, in that order, the inbox with its count', (tester) async {
      store = Store(AppState.initial, reduce, (_, _) {});
      await tester.pumpWidget(StoreScope(
        store: store,
        child: const MaterialApp(home: Scaffold(bottomNavigationBar: PhoneTabs(tab: PhoneTab.workflows, needsYou: 2))),
      ));
      final labels = [for (final key in ['inbox', 'workflows', 'pages', 'server']) tester.getTopLeft(find.byKey(Key('tab-$key'))).dx];
      expect(labels, [...labels]..sort());
      expect(find.text('inbox · 2'), findsOneWidget);
      expect(find.text('workflows'), findsOneWidget);
      expect(find.text('pages'), findsOneWidget);
      expect(find.text('server'), findsOneWidget);
      for (final key in ['inbox', 'workflows', 'pages', 'server']) {
        expect(tester.getSize(find.byKey(Key('tab-$key'))).height, greaterThanOrEqualTo(44));
      }
    });

    testWidgets('no count when nothing needs you; a tap selects the tab', (tester) async {
      store = Store(AppState.initial, reduce, (_, _) {});
      await tester.pumpWidget(StoreScope(
        store: store,
        child: MaterialApp(home: Scaffold(bottomNavigationBar: PhoneTabs(tab: PhoneTab.inbox, needsYou: 0))),
      ));
      expect(find.text('inbox'), findsOneWidget);
      await tester.tap(find.byKey(const Key('tab-workflows')));
      expect(store.state.tab, PhoneTab.workflows);
      await tester.tap(find.byKey(const Key('tab-server')));
      expect(store.state.tab, PhoneTab.server);
    });
  });

  group('the Workflows tab', () {
    Future<void> pumpTab(WidgetTester tester) async {
      await tester.binding.setSurfaceSize(const Size(400, 900));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(StoreScope(
        store: store,
        child: MaterialApp(
          home: Scaffold(
            body: Builder(builder: (context) => WorkflowsTab(url: home.url, status: StoreScope.of(context).state.statusOf(home.url))),
          ),
        ),
      ));
      await tester.pump();
      await tester.pump();
    }

    testWidgets('rows with the application, MCP servers and sessions; plain sites are left out', (tester) async {
      setUp();
      store.dispatch(ServerEvent(home.url, ServerMessageSessionUpdated(session: session('b', site: '3d'))));
      store.dispatch(ServerEvent(home.url, ServerMessageSessionUpdated(session: session('c', site: '3d'))));
      await pumpTab(tester);
      expect(find.byKey(const Key('workflow-3d')), findsOneWidget);
      expect(find.byKey(const Key('workflow-docs')), findsOneWidget);
      expect(find.byKey(const Key('workflow-plain')), findsNothing);
      expect(find.text('blender'), findsOneWidget);
      expect(find.text('search'), findsOneWidget);
      expect(find.text('2 sessions'), findsOneWidget);
      expect(find.text('no sessions'), findsOneWidget);
      expect(find.byKey(const Key('workflow-session-b')), findsOneWidget);
      expect(find.byKey(const Key('workflow-session-c')), findsOneWidget);
    });

    testWidgets('asks for the catalog again when it shows', (tester) async {
      setUp();
      await pumpTab(tester);
      expect(effects.whereType<FetchCatalog>(), hasLength(1));
    });

    testWidgets('the new-chat action opens the sheet for that workflow, and Start creates the chat there', (tester) async {
      setUp();
      await pumpTab(tester);
      expect(tester.getSize(find.byKey(const Key('workflow-new-3d'))), const Size(44, 44));
      await tester.tap(find.byKey(const Key('workflow-new-3d')));
      await tester.pumpAndSettle();
      expect(find.text('New chat in 3d'), findsOneWidget);
      await tester.tap(find.byKey(const Key('start-chat')));
      await tester.pump();
      final create = effects.whereType<CreateSession>().single;
      expect((create.agent, create.site), ('default', '3d'));
    });

    testWidgets('says what makes a site a workflow when there are none', (tester) async {
      setUp();
      store.dispatch(SitesLoaded(home.url, [site('plain')]));
      await pumpTab(tester);
      expect(find.byKey(const Key('workflows-empty')), findsOneWidget);
      expect(find.textContaining('app:', findRichText: true), findsOneWidget);
      expect(find.textContaining('mcp:', findRichText: true), findsOneWidget);
      expect(find.textContaining('index.md', findRichText: true), findsOneWidget);
    });
  });

  group('the chat order', () {
    Future<void> pumpChat(WidgetTester tester, {required bool newestFirst}) async {
      setUp();
      await tester.binding.setSurfaceSize(const Size(400, 800));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(StoreScope(
        store: store,
        child: MaterialApp(
          home: Scaffold(
            body: Builder(builder: (context) {
              final status = StoreScope.of(context).state.statusOf(home.url);
              return ChatView(url: home.url, status: status, chat: status.chat!, newestFirst: newestFirst);
            }),
          ),
        ),
      ));
      await tester.pump();
    }

    testWidgets('newest first: the newest message is at the top, older below', (tester) async {
      await pumpChat(tester, newestFirst: true);
      expect(tester.getTopLeft(find.text('newer')).dy, lessThan(tester.getTopLeft(find.text('older')).dy));
    });

    testWidgets('conventional: the newest message is at the bottom', (tester) async {
      await pumpChat(tester, newestFirst: false);
      expect(tester.getTopLeft(find.text('newer')).dy, greaterThan(tester.getTopLeft(find.text('older')).dy));
    });

    testWidgets('a pending approval comes first in newest-first order, last (nearest the bar) in the other', (tester) async {
      for (final newestFirst in [true, false]) {
        await pumpChat(tester, newestFirst: newestFirst);
        store.dispatch(ServerEvent(
          home.url,
          const ServerMessageApprovalRequest(request: 'r1', session: 'a', site: 'tools', recipe: 'greet', args: [], body: 'greet bob', hash: 'h'),
        ));
        await tester.pump();
        final approval = tester.getTopLeft(find.byKey(const Key('approve-r1'))).dy;
        final newest = tester.getTopLeft(find.text('newer')).dy;
        expect(approval < newest, newestFirst, reason: 'newestFirst=$newestFirst');
        expect(find.byKey(const Key('always-r1')).evaluate().length + find.byKey(const Key('deny-r1')).evaluate().length, greaterThanOrEqualTo(1));
      }
    });
  });
}
