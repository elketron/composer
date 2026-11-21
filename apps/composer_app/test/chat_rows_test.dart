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
      tool('open', 'site://plans'),
      tool('write', 'site://plans/tickets/0001.md'),
      tool('run', 'plans › ui-build', error: true),
      const TextItem('Done.'),
      const ThinkingItem('hm'),
    ]);
    expect(rows.map((r) => r.runtimeType), [ItemRow, StepsRow, ItemRow, StepsRow]);
    final steps = rows[1] as StepsRow;
    expect(steps.steps, hasLength(4));
    expect(steps.summary, 'read 1 page, wrote 0001.md, ran ui-build, 1 failed');
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
}
