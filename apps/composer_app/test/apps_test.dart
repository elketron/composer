// Local apps on the desk (`[apps.<name>]`, `/apps/<name>/`): the address an app tab loads, which
// links stay in its webview, its label, the desk the phone keeps for the open session, opening one
// by hand, and the readable state when the app is down.

import 'package:composer_app/flux/store.dart';
import 'package:composer_app/state/actions.dart';
import 'package:composer_app/state/app_state.dart';
import 'package:composer_app/state/apps.dart';
import 'package:composer_app/state/chat_state.dart';
import 'package:composer_app/state/effects.dart';
import 'package:composer_app/state/reduce.dart';
import 'package:composer_app/state/server_entry.dart';
import 'package:composer_app/state/server_status.dart';
import 'package:composer_app/theme/composer_theme.dart';
import 'package:composer_app/ui/apps/app_screen.dart';
import 'package:composer_app/ui/apps/app_tabs_row.dart';
import 'package:composer_app/ui/apps/app_view.dart';
import 'package:composer_app/ui/store_scope.dart';
import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

const url = 'https://home.ts.net';
const server = ServerEntry(url: url, name: 'home', token: 't1');
const info = ServerInfo(
  name: 'home-pc',
  version: '0.1.0',
  model: 'qwen',
  slots: 2,
  apps: [AppInfo(name: 'canvas', title: 'Agent canvas'), AppInfo(name: 'plain', title: '')],
);

DeskEntry app(String name, {String? path, bool unseen = false}) =>
    DeskEntry(id: 'app:$name', tab: DeskTabApp(name: name, path: path), unseen: unseen);

AppState withChat(Desk desk) => AppState(
      servers: const [server],
      status: {url: ServerStatus(info: info, chat: ChatState(session: '7', loaded: true, desk: desk))},
    );

void main() {
  group('the address', () {
    test('is the app under /apps/<name>/ at its path', () {
      expect(appUrl(url, 'canvas', null), 'https://home.ts.net/apps/canvas/');
      expect(appUrl(url, 'canvas', ''), 'https://home.ts.net/apps/canvas/');
      expect(appUrl(url, 'canvas', 'board/2'), 'https://home.ts.net/apps/canvas/board/2');
      expect(appUrl(url, 'canvas', '/board/2?zoom=1#n3'), 'https://home.ts.net/apps/canvas/board/2?zoom=1#n3');
    });

    test('keeps the workspace path and port, never a token', () {
      expect(appUrl('http://10.0.2.2:7431/', 'test', 'x'), 'http://10.0.2.2:7431/apps/test/x');
      expect(appUrl('https://pc.ts.net/composer?token=s', 'test', ''), 'https://pc.ts.net/composer/apps/test/');
    });

    test('only the app\'s own pages stay in its webview', () {
      bool inside(String target) => insideApp(url, 'canvas', Uri.parse(target));
      expect(inside('https://home.ts.net/apps/canvas/'), isTrue);
      expect(inside('https://home.ts.net/apps/canvas'), isTrue);
      expect(inside('https://home.ts.net/apps/canvas/board/2?x=1'), isTrue);
      expect(inside('https://home.ts.net/apps/canvasx/'), isFalse);
      expect(inside('https://home.ts.net/apps/other/'), isFalse);
      expect(inside('https://home.ts.net/view'), isFalse);
      expect(inside('http://home.ts.net/apps/canvas/'), isFalse);
      expect(inside('https://home.ts.net:8443/apps/canvas/'), isFalse);
      expect(inside('https://example.com/apps/canvas/'), isFalse);
    });
  });

  test('the label is the app\'s title, else its name', () {
    expect(appTitle(info, 'canvas'), 'Agent canvas');
    expect(appTitle(info, 'plain'), 'plain');
    expect(appTitle(info, 'gone'), 'gone');
    expect(appTitle(null, 'canvas'), 'canvas');
  });

  test('the app is down: said so, with the status', () {
    final down = appFailure('Agent canvas', status: 502);
    expect(down.headline, 'Agent canvas is not running');
    expect(down.detail, contains('502'));
    expect(appFailure('Agent canvas', status: 500).headline, 'Agent canvas could not load');
    expect(appFailure('Agent canvas', error: 'net::ERR_CONNECTION_REFUSED').detail, 'net::ERR_CONNECTION_REFUSED');
  });

  group('the desk', () {
    test('comes with the attach and follows desk.changed of the open session only', () {
      final open = reduce(AppState(servers: const [server], status: {url: const ServerStatus(info: info)}), const SessionOpened(url, '7')).$1;
      final (_, effects) = reduce(open, const HistoryLoaded(url, '7', null, []));
      expect(effects.whereType<DeskCommand>().single.command, isA<ClientCommandDeskGet>());

      var state = reduce(withChat(const Desk()), DeskLoaded(url, '7', Desk(tabs: [app('canvas')]))).$1;
      expect(appTabs(state.statusOf(url).chat!.desk).single.$2.name, 'canvas');

      final moved = Desk(tabs: [app('canvas', path: 'board/2', unseen: true)]);
      state = reduce(state, ServerEvent(url, ServerMessageDeskChanged(session: '7', desk: moved, by: DeskBy.agent, opened: 'app:canvas'))).$1;
      expect(appTab(state.statusOf(url).chat!.desk, 'canvas')!.path, 'board/2');

      state = reduce(state, const ServerEvent(url, ServerMessageDeskChanged(session: '8', desk: Desk(), by: DeskBy.user))).$1;
      expect(appTabs(state.statusOf(url).chat!.desk), hasLength(1));
    });

    test('opening an app by hand puts it on the desk, or marks the agent\'s seen', () {
      DeskCommand? sent(Desk desk, String name) => reduce(withChat(desk), AppOpened(url, name)).$2.whereType<DeskCommand>().firstOrNull;

      final open = sent(const Desk(), 'canvas')!.command as ClientCommandDeskOpen;
      expect(open.tab.toJson(), {'kind': 'app', 'name': 'canvas', 'path': ''});
      expect(open.focus, isFalse);

      final focus = sent(Desk(tabs: [app('canvas', unseen: true)]), 'canvas')!.command as ClientCommandDeskFocus;
      expect(focus.tab, 'app:canvas');

      expect(sent(Desk(tabs: [app('canvas')]), 'canvas'), isNull);
    });
  });

  group('widgets', () {
    Future<Store<AppState, AppAction, AppEffect>> pump(WidgetTester tester, Desk desk, Widget Function() child) async {
      final store = Store<AppState, AppAction, AppEffect>(withChat(desk), reduce, (_, _) async {});
      await tester.pumpWidget(StoreScope(store: store, child: MaterialApp(theme: composerTheme(), home: Scaffold(body: child()))));
      return store;
    }

    Widget fakeView(BuildContext context, AppViewRequest r) => Text('view ${r.name} at "${r.path}" as ${r.title}', key: const Key('fake-app-view'));

    testWidgets('the row labels each app tab with its title and badges an unseen one', (tester) async {
      final desk = Desk(tabs: [
        const DeskEntry(id: 'page:site://plans/x', tab: DeskTabPage(url: 'site://plans/x')),
        app('canvas', unseen: true),
        app('plain'),
      ]);
      await pump(tester, desk, () => Builder(builder: (c) => AppTabsRow(url: url, status: StoreScope.of(c).state.statusOf(url), viewBuilder: fakeView)));
      expect(find.text('Agent canvas'), findsOneWidget);
      expect(find.text('plain'), findsOneWidget);
      expect(find.descendant(of: find.byKey(const Key('app-tab-canvas')), matching: find.byKey(const Key('app-tab-unseen'))), findsOneWidget);
      expect(find.descendant(of: find.byKey(const Key('app-tab-plain')), matching: find.byKey(const Key('app-tab-unseen'))), findsNothing);

      await tester.tap(find.byKey(const Key('app-tab-canvas')));
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('app-title')), findsOneWidget);
      expect(tester.widget<Text>(find.byKey(const Key('app-title'))).data, 'Agent canvas');
    });

    testWidgets('no app tabs: no row', (tester) async {
      await pump(tester, const Desk(), () => Builder(builder: (c) => AppTabsRow(url: url, status: StoreScope.of(c).state.statusOf(url))));
      expect(find.byKey(const Key('app-tabs')), findsNothing);
    });

    testWidgets('the screen shows the app at its tab\'s path and follows it', (tester) async {
      final store = await pump(tester, Desk(tabs: [app('canvas', path: 'board/1')]), () => const AppScreen(url: url, name: 'canvas', viewBuilder: _fake));
      expect(find.text('view canvas at "board/1" as Agent canvas'), findsOneWidget);
      store.dispatch(ServerEvent(url, ServerMessageDeskChanged(session: '7', desk: Desk(tabs: [app('canvas', path: 'board/2')]), by: DeskBy.agent, opened: 'app:canvas')));
      await tester.pump();
      expect(find.text('view canvas at "board/2" as Agent canvas'), findsOneWidget);
    });

    testWidgets('a path-less tab is the app\'s page', (tester) async {
      await pump(tester, Desk(tabs: [app('plain')]), () => const AppScreen(url: url, name: 'plain', viewBuilder: _fake));
      expect(find.text('view plain at "" as plain'), findsOneWidget);
    });

    testWidgets('the down state reads and reloads', (tester) async {
      var reloaded = 0;
      final down = appFailure('Agent canvas', status: 502);
      await tester.pumpWidget(MaterialApp(home: Scaffold(body: AppUnavailable(headline: down.headline, detail: down.detail, onReload: () => reloaded++))));
      expect(find.text('Agent canvas is not running'), findsOneWidget);
      await tester.tap(find.byKey(const Key('app-reload')));
      expect(reloaded, 1);
    });
  });
}

Widget _fake(BuildContext context, AppViewRequest r) => Text('view ${r.name} at "${r.path}" as ${r.title}');
