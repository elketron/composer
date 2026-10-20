// What a tool call shows: its interesting argument on one line, and (for edits and to-do lists)
// what it changes. Composer's own tools and Claude Code's; the TUI's tool_text.rs, in Dart.

import 'dart:convert';

/// Lines of a written file shown in the details.
const _writeLines = 40;

/// The interesting argument of a call, on one line.
String toolSummary(String name, Object? args) {
  if (args is! Map) return args == null ? '' : jsonEncode(args);
  String? get(String key) => args[key] is String ? args[key] as String : null;
  final summary = switch (name) {
    'Read' || 'Write' || 'Edit' || 'MultiEdit' || 'NotebookEdit' => _short(get('file_path') ?? get('notebook_path')),
    'Bash' => get('command')?.split('\n').first,
    'Grep' || 'Glob' => get('pattern') == null ? null : [get('pattern'), if (get('path') != null) 'in ${_short(get('path'))}'].join(' '),
    'Agent' || 'Task' => get('description'),
    'Skill' => get('skill'),
    'TaskCreate' => get('subject'),
    'TaskUpdate' => get('taskId') == null ? null : '#${get('taskId')} ${get('status') ?? 'updated'}',
    'TodoWrite' => () {
        final todos = args['todos'] is List ? (args['todos'] as List).whereType<Map>().toList() : const <Map>[];
        return '${todos.where((t) => t['status'] == 'completed').length}/${todos.length} done';
      }(),
    _ => null,
  };
  return summary ?? summarizeArgs(args);
}

/// What the call changes: an edit's diff (`-` / `+` lines), a written file, a to-do list.
String? toolDetail(String name, Object? args) {
  if (args is! Map) return null;
  String diff(Object? edit) {
    if (edit is! Map) return '';
    final old = edit['old_string'] is String ? edit['old_string'] as String : '';
    final next = edit['new_string'] is String ? edit['new_string'] as String : '';
    return [for (final l in const LineSplitter().convert(old)) '- $l', for (final l in const LineSplitter().convert(next)) '+ $l'].join('\n');
  }

  switch (name) {
    case 'Edit':
      return diff(args);
    case 'MultiEdit':
      return args['edits'] is List ? (args['edits'] as List).map(diff).join('\n\n') : null;
    case 'Write':
      if (args['content'] is! String) return null;
      final lines = const LineSplitter().convert(args['content'] as String);
      return [
        for (final l in lines.take(_writeLines)) '+ $l',
        if (lines.length > _writeLines) '… ${lines.length - _writeLines} more lines',
      ].join('\n');
    case 'TodoWrite':
      if (args['todos'] is! List) return null;
      return [
        for (final t in (args['todos'] as List).whereType<Map>())
          '${switch (t['status']) { 'completed' => '[x]', 'in_progress' => '[~]', _ => '[ ]' }} ${t['content'] ?? ''}',
      ].join('\n');
  }
  return null;
}

/// A path inside a ticket's worktree, from the worktree (`src/main.rs`); others as they are.
String? _short(String? path) {
  if (path == null) return null;
  final i = path.indexOf('/worktrees/');
  if (i < 0) return path;
  final rest = path.substring(i + '/worktrees/'.length);
  final slash = rest.indexOf('/');
  return slash < 0 ? rest : rest.substring(slash + 1);
}

/// Composer's tools: the URL, query, recipe… of the call (as the TUI shows it).
String summarizeArgs(Map args) {
  String? get(String key) => args[key] is String ? args[key] as String : null;
  final site = get('site'), recipe = get('recipe');
  if (site != null && recipe != null) {
    final extra = (args['args'] is List ? args['args'] as List : const []).whereType<String>().join(' ');
    return '$site › $recipe $extra'.trimRight();
  }
  for (final key in ['url', 'query', 'path', 'ticket', 'status', 'site']) {
    final value = get(key);
    if (value != null) return value;
  }
  return args.isEmpty ? '' : jsonEncode(args);
}
