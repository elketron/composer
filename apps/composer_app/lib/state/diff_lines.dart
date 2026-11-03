import 'package:composer_protocol/composer_protocol.dart';

enum DiffLineKind { file, hunk, add, del, context, meta }

/// One line of the diff view. `line` is what a comment on it anchors to (`file:start:end`): the
/// new file's line number, for a removed line where it sits in the new file; null where nothing
/// can be commented.
class DiffLine {
  const DiffLine(this.kind, this.text, {this.file = '', this.line, this.quote});
  final DiffLineKind kind;

  /// The line as shown (for a text diff: with its `+`/`-`/space marker).
  final String text;
  final String file;
  final int? line;

  /// The code of the line, without the diff's marker (a removed line keeps its `-`).
  final String? quote;

  bool get commentable => line != null;
}

final _hunk = RegExp(r'^@@ -\d+(?:,\d+)? \+(\d+)(?:,\d+)? @@');
final _difftHeader = RegExp(r'^(.*) --- \S.*$');

/// The unified diff as lines, each knowing its file and line number.
List<DiffLine> parseUnifiedDiff(String diff) {
  final out = <DiffLine>[];
  var file = '';
  var newLine = 0;
  var inHunk = false;
  for (final raw in diff.split('\n')) {
    if (raw.startsWith('diff --git ')) {
      inHunk = false;
      final m = RegExp(r' b/(.*)$').firstMatch(raw);
      file = m?[1] ?? '';
      out.add(DiffLine(DiffLineKind.file, file, file: file));
    } else if (!inHunk && (raw.startsWith('--- ') || raw.startsWith('+++ ') || raw.startsWith('new file') || raw.startsWith('deleted file') || raw.startsWith('index ') || raw.startsWith('Binary'))) {
      out.add(DiffLine(DiffLineKind.meta, raw, file: file));
    } else if (_hunk.firstMatch(raw) case final m?) {
      inHunk = true;
      newLine = int.parse(m[1]!);
      out.add(DiffLine(DiffLineKind.hunk, raw, file: file, line: newLine, quote: raw));
    } else if (inHunk && raw.startsWith('+')) {
      out.add(DiffLine(DiffLineKind.add, raw, file: file, line: newLine++, quote: raw.substring(1).trim()));
    } else if (inHunk && raw.startsWith('-')) {
      out.add(DiffLine(DiffLineKind.del, raw, file: file, line: newLine < 1 ? 1 : newLine, quote: '-${raw.substring(1).trim()}'));
    } else if (inHunk && raw.startsWith(' ')) {
      out.add(DiffLine(DiffLineKind.context, raw, file: file, line: newLine++, quote: raw.substring(1).trim()));
    } else if (raw.isNotEmpty) {
      out.add(DiffLine(DiffLineKind.meta, raw, file: file));
    }
  }
  return out;
}

/// difft's plain output for one file as lines. Its gutter is two line-number cells (old, new) of
/// the same width (the digits of the biggest number, at least 2) and a space; an unchanged line
/// shows one number, a changed one the number on its side. A comment anchors to the number
/// shown (the new file's where there is one); difft's plain output doesn't say which lines were
/// removed or added, so none is coloured.
List<DiffLine> parseSemanticDiff(String file, String semantic) {
  final rows = semantic.split('\n');
  final body = rows.isNotEmpty && _difftHeader.hasMatch(rows.first) ? rows.sublist(1) : rows;
  var width = 2;
  for (final row in body) {
    for (final m in RegExp(r'^(\s*\d+)(\s+\d+)?').allMatches(row)) {
      final digits = [m[1], m[2]].whereType<String>().map((x) => x.trim().length);
      if (digits.any((d) => d > width)) width = digits.reduce((a, b) => a > b ? a : b);
    }
  }
  final out = <DiffLine>[];
  for (final row in body) {
    if (row.trim().isEmpty) continue;
    final cells = row.length >= 2 * width ? row.substring(0, 2 * width) : row;
    final old = int.tryParse(cells.substring(0, width.clamp(0, cells.length)).trim());
    final now = cells.length > width ? int.tryParse(cells.substring(width).trim()) : null;
    final line = now ?? old;
    final text = row.length > 2 * width + 1 ? row.substring(2 * width + 1) : '';
    out.add(DiffLine(line == null ? DiffLineKind.meta : DiffLineKind.context, row, file: file, line: line, quote: line == null ? null : text.trim()));
  }
  return out;
}

/// The lines the diff view shows: per file its header, then the structural diff when `semantic`
/// and the server sent one for it, else its hunks from the text diff.
List<DiffLine> diffViewLines(ProjectDiff changes, {required bool semantic}) {
  final text = parseUnifiedDiff(changes.diff);
  final out = <DiffLine>[];
  final seen = <String>{};
  for (var i = 0; i < text.length; i++) {
    final line = text[i];
    if (line.kind != DiffLineKind.file) continue;
    seen.add(line.file);
    out.add(line);
    final end = text.indexWhere((l) => l.kind == DiffLineKind.file, i + 1);
    final sem = semantic ? changes.files.where((f) => f.path == line.file).firstOrNull?.semantic : null;
    if (sem != null) {
      out.addAll(parseSemanticDiff(line.file, sem));
    } else {
      out.addAll(text.sublist(i + 1, end < 0 ? text.length : end));
    }
  }
  return out;
}

/// What a comment on the lines `from..to` (either order) anchors to: the file, the first and
/// last line number, the quote (several lines are joined by `\n`). Null where there is nothing
/// to comment on, or the lines span files. Rows with nothing to anchor to are skipped.
DiffComment? diffAnchor(List<DiffLine> lines, int from, int to, String message) {
  final (a, b) = from <= to ? (from, to) : (to, from);
  String? file;
  final numbers = <int>[];
  final quotes = <String>[];
  for (final line in lines.sublist(a, b + 1)) {
    if (!line.commentable) continue; // headers and notes inside a range are skipped
    if (file != null && file != line.file) return null;
    file = line.file;
    numbers.add(line.line!);
    quotes.add(line.quote ?? '');
  }
  if (file == null) return null;
  final quote = quotes.join('\n');
  return DiffComment(
    file: file,
    start: numbers.reduce((x, y) => x < y ? x : y),
    end: numbers.reduce((x, y) => x > y ? x : y),
    message: message,
    quote: quote.isEmpty ? null : quote,
  );
}
