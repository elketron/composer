import 'package:composer_app/state/chat_items.dart';
import 'package:composer_app/state/chat_rows.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  ToolItem tool(String name, String summary, {bool error = false}) =>
      ToolItem(callId: name + summary, name: name, summary: summary, result: 'ok', isError: error);

  test('a turn\'s steps fold into one row between messages', () {
    final rows = chatRows([
      const UserItem('write ticket 0001'),
      const ThinkingItem('check the format'),
      tool('open', 'site://orchestration'),
      tool('write', 'site://orchestration/tickets/0001.md'),
      tool('spawn_worker', '0001'),
      tool('run', 'orchestration › ui-build', error: true),
      const TextItem('Done.'),
      const ThinkingItem('hm'),
    ]);
    expect(rows.map((r) => r.runtimeType), [ItemRow, StepsRow, ItemRow, StepsRow]);
    final steps = rows[1] as StepsRow;
    expect(steps.steps, hasLength(5));
    expect(steps.summary, 'read 1 page, wrote 0001.md, started a worker, ran ui-build, 1 failed');
    expect((rows[3] as StepsRow).summary, 'hm', reason: 'only thoughts: the thought itself');
  });

  test('the row a streaming reply will land in', () {
    // Nothing of its own folded in front: it is the row appended at the end.
    expect(pendingRowIndex([const UserItem('hi')], thinking: false), 1);
    // A thinking block of its own folds a row before it.
    expect(pendingRowIndex([const UserItem('hi')], thinking: true), 2);
    // A tool call that already arrived folds together with its thinking, not in front of it.
    expect(pendingRowIndex([const UserItem('hi'), tool('open', 'a')], thinking: true), 2);
    // Rows older messages prepended are counted in (the caller takes rowBase off).
    expect(pendingRowIndex([const UserItem('old'), const TextItem('answer'), tool('open', 'a')], thinking: false), 3);
  });

  test('summaries count repeats', () {
    expect(stepsSummary([tool('open', 'a'), tool('open', 'b'), tool('edit', 'x/a.md'), tool('edit', 'x/b.md')]), 'read 2 pages, edited 2 pages');
  });

  test('a worker\'s report becomes a card', () {
    const report = UserItem(
      '[worker #96a1 finished ticket 0001: done]\nwrote the release page\n\nIts Result is in site://orchestration/tickets/0001.md.',
      source: 'worker #96a1',
    );
    final row = chatRows([report]).single as WorkerRow;
    expect((row.worker, row.ticket, row.status, row.summary, row.result),
        ('96a1', '0001', 'done', 'wrote the release page', 'site://orchestration/tickets/0001.md'));
    // With the branch its changes are on.
    const onBranch = UserItem(
      '[worker #96a1 finished ticket 0001: paused]\nusage limit\n\nIts Result is in site://orchestration/tickets/0001.md. Its changes are on branch `ticket/0001`.',
      source: 'worker #96a1',
    );
    final paused = chatRows([onBranch]).single as WorkerRow;
    expect((paused.status, paused.summary, paused.result), ('paused', 'usage limit', 'site://orchestration/tickets/0001.md'));
    expect((row.branch, paused.branch), (null, 'ticket/0001'));
    expect(chatRows([const UserItem('[worker #1 finished ticket 2: done]')]).single, isA<ItemRow>(), reason: 'yours, not a worker\'s');
  });
}
