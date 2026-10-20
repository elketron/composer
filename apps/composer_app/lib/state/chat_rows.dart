// Folding a chat's items into rows (pure, tested).

import 'chat_items.dart';

/// Tool calls and thoughts between messages become one `StepsRow`; workers' reports a `WorkerRow`.
List<ChatRow> chatRows(List<ChatItem> items) {
  final rows = <ChatRow>[];
  var steps = <ChatItem>[];
  void flush() {
    if (steps.isEmpty) return;
    rows.add(StepsRow(steps, stepsSummary(steps)));
    steps = [];
  }

  for (final item in items) {
    if (item is ToolItem || item is ThinkingItem) {
      steps.add(item);
      continue;
    }
    flush();
    rows.add(item is UserItem ? (workerReport(item) ?? ItemRow(item)) : ItemRow(item));
  }
  flush();
  return rows;
}

/// `read 2 pages, wrote 0001.md, started a worker`: what the steps did, in the order first done.
String stepsSummary(List<ChatItem> steps) {
  final tools = steps.whereType<ToolItem>().toList();
  // Only thoughts: the start of the last one.
  if (tools.isEmpty) {
    final thought = steps.whereType<ThinkingItem>().lastOrNull?.text.trim() ?? '';
    return thought.split('\n').first;
  }
  final groups = <String, List<ToolItem>>{};
  for (final tool in tools) {
    groups.putIfAbsent(tool.name, () => []).add(tool);
  }
  String files(List<ToolItem> t, String verb) =>
      t.length == 1 ? '$verb ${_file(t.single.summary)}' : '$verb ${t.length} pages';
  final parts = [
    for (final MapEntry(key: name, value: t) in groups.entries)
      switch (name) {
        'open' => 'read ${_count(t.length, 'page')}',
        'search' => t.length == 1 ? 'searched' : 'searched ${t.length} times',
        'list_sites' => 'listed the sites',
        'write' => files(t, 'wrote'),
        'edit' => files(t, 'edited'),
        'spawn_worker' => 'started ${t.length == 1 ? 'a worker' : '${t.length} workers'}',
        'run' => t.length == 1 ? 'ran ${t.single.summary.split(' › ').last}' : 'ran ${t.length} recipes',
        'actions' => 'looked at actions',
        'set_project' => 'set the project',
        'finish' => 'finished',
        _ => t.length == 1 ? name.replaceAll('_', ' ') : '${name.replaceAll('_', ' ')} ×${t.length}',
      },
  ];
  final failed = tools.where((t) => t.isError).length;
  return [...parts, if (failed > 0) '$failed failed'].join(', ');
}

String _count(int n, String noun) => '$n $noun${n == 1 ? '' : 's'}';

/// `site://orchestration/tickets/0001.md` → `0001.md`.
String _file(String summary) => summary.split('/').where((s) => s.isNotEmpty).lastOrNull ?? summary;

final _report = RegExp(r'^\[worker #(\S+) finished ticket (\S+): (\w+)\]\s*(.*?)(?:\s*Its Result is in (site://\S+?)\.?)?\s*$', dotAll: true);

/// A finished worker's message to its orchestrator, if `item` is one.
WorkerRow? workerReport(UserItem item) {
  if (!(item.source?.startsWith('worker') ?? false)) return null;
  final m = _report.firstMatch(item.text);
  if (m == null) return null;
  return WorkerRow(worker: m[1]!, ticket: m[2]!, status: m[3]!, summary: m[4]!.trim(), result: m[5]);
}
