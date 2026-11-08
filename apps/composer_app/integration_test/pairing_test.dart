// Pairs with a real server through the UI, then another workspace of the same host and switches
// between them. Run on the emulator by scripts/phone-e2e.sh (it makes the pairing codes).

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'support.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('two workspaces of one host, paired and switched', (tester) async {
    await startPaired(tester);
    await tester.tap(find.byKey(const Key('tab-server')));
    await pumpUntil(tester, find.textContaining('online'));
    expect(find.textContaining(' in use'), findsOneWidget);
    await tester.tap(find.byKey(const Key('tab-chats')));

    if (pairLine2.isEmpty) return;
    final (first, second) = workspaceUrls();
    await pairAnother(tester, pairLine2);

    // The switcher lists them under one host.
    final sheet = find.byKey(Key('host-${hostOf(first)}'));
    if (sheet.evaluate().isEmpty) {
      await tester.tap(find.byKey(const Key('switch-server')));
    }
    await pumpUntil(tester, sheet);
    expect(find.byKey(Key('workspace-$first')), findsOneWidget);
    expect(find.byKey(Key('workspace-$second')), findsOneWidget);

    // Switching to the first shows its name at the top.
    await tester.tap(find.byKey(Key('workspace-$first')));
    await pumpUntil(tester, find.byKey(const Key('active-workspace')));
    expect(
      find.descendant(of: find.byKey(const Key('active-workspace')), matching: find.text(workspaceInUrl(first))),
      findsOneWidget,
    );

    // And to the second.
    await tester.tap(find.byKey(const Key('switch-server')));
    await pumpUntil(tester, find.byKey(Key('workspace-$second')));
    await tester.tap(find.byKey(Key('workspace-$second')));
    await pumpUntil(tester, find.byKey(const Key('active-workspace')));
    expect(
      find.descendant(of: find.byKey(const Key('active-workspace')), matching: find.text(workspaceInUrl(second))),
      findsOneWidget,
    );
  });
}
