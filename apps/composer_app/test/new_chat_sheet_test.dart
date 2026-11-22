// New chat on a small phone with many sites: every site chip can be reached by scrolling, Start
// stays on screen, and the selected chip's label is in the full text colour.

import 'package:composer_app/flux/store.dart';
import 'package:composer_app/state/app_state.dart';
import 'package:composer_app/state/reduce.dart';
import 'package:composer_app/theme/composer_theme.dart';
import 'package:composer_app/theme/tokens.dart' as t;
import 'package:composer_app/state/server_status.dart';
import 'package:composer_app/ui/home/new_chat_sheet.dart';
import 'package:composer_app/ui/store_scope.dart';
import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

const url = 'https://home.ts.net';
const names = ['3d', 'agents', 'chat', 'coding', 'home', 'icm', 'ideas', 'memory', 'mockups', 'orchestration', 'plans', 'projects', 'talk', 'video'];

void main() {
  testWidgets('every site is reachable and Start stays visible', (tester) async {
    await tester.binding.setSurfaceSize(const Size(360, 640));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final sites = [for (final n in names) SiteSummary(name: n, title: n, hasJustfile: false)];
    final store = Store(AppState(status: {url: ServerStatus(sites: sites)}), reduce, (_, _) async {});
    late BuildContext root;
    await tester.pumpWidget(StoreScope(
      store: store,
      child: MaterialApp(
        theme: composerTheme(),
        home: Builder(builder: (c) {
          root = c;
          return const Scaffold();
        }),
      ),
    ));
    showNewChat(root, url);
    await tester.pumpAndSettle();

    final start = find.byKey(const Key('start-chat'));
    bool onScreen(Finder f) => tester.getRect(f).bottom <= 640 && tester.getRect(f).top >= 0;
    expect(onScreen(start), isTrue);

    final video = find.byKey(const Key('new-chat-site-video'));
    await tester.scrollUntilVisible(video, 50, scrollable: find.descendant(of: find.byKey(const Key('new-chat-scroll')), matching: find.byType(Scrollable)));
    await tester.pumpAndSettle();
    expect(tester.getRect(video).bottom, lessThanOrEqualTo(tester.getRect(start).top));
    expect(onScreen(start), isTrue);

    await tester.tap(video);
    await tester.pumpAndSettle();
    final label = tester.widget<Text>(find.descendant(of: video, matching: find.text('video')));
    expect(label.style?.color, t.text);
    final none = tester.widget<Text>(find.descendant(of: find.byKey(const Key('new-chat-site-none')), matching: find.text('no site')));
    expect(none.style?.color, t.muted);
  });
}
