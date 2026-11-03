// The diff view's parsing: unified diffs and difft's output into lines that know their anchor.

import 'package:composer_app/state/diff_lines.dart';
import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter_test/flutter_test.dart';

const unified = '''diff --git a/a.rs b/a.rs
--- a/a.rs
+++ b/a.rs
@@ -1,3 +1,4 @@
 fn a() {
-    let x = 1;
+    let x = 2;
+    bar();
 }
diff --git a/n.txt b/n.txt
new file
--- /dev/null
+++ b/n.txt
@@ -0,0 +1,1 @@
+plain
''';

const difft = '''a.rs --- Rust
1    fn a() {
2        let x = 1;
   2     let x = 2;
   4     bar();
   5 }
''';

void main() {
  test('unified diff lines carry file and line numbers', () {
    final lines = parseUnifiedDiff(unified).where((l) => l.commentable).toList();
    expect([for (final l in lines) (l.file, l.kind, l.line, l.quote)], [
      ('a.rs', DiffLineKind.hunk, 1, '@@ -1,3 +1,4 @@'),
      ('a.rs', DiffLineKind.context, 1, 'fn a() {'),
      ('a.rs', DiffLineKind.del, 2, '-    let x = 1;'),
      ('a.rs', DiffLineKind.add, 2, '    let x = 2;'),
      ('a.rs', DiffLineKind.add, 3, '    bar();'),
      ('a.rs', DiffLineKind.context, 4, '}'),
      ('n.txt', DiffLineKind.hunk, 1, '@@ -0,0 +1,1 @@'),
      ('n.txt', DiffLineKind.add, 1, 'plain'),
    ]);
  });

  test('difft output anchors to the number its gutter shows', () {
    final lines = parseSemanticDiff('a.rs', difft);
    expect([for (final l in lines) (l.line, l.quote)], [
      (1, 'fn a() {'),
      (2, 'let x = 1;'),
      (2, 'let x = 2;'),
      (4, 'bar();'),
      (5, '}'),
    ]);
    const wide = ' 98    l98\n100    l100\n   100 changed\n   101 l101\n';
    expect([for (final l in parseSemanticDiff('b.py', 'b.py --- Python\n$wide')) (l.line, l.quote)], [
      (98, 'l98'),
      (100, 'l100'),
      (100, 'changed'),
      (101, 'l101'),
    ]);
  });

  test('the structural diff replaces a file\'s hunks only when the server sent one', () {
    final changes = ProjectDiff(
      path: '/wt',
      files: const [
        ChangedFile(path: 'a.rs', status: 'M', added: 2, removed: 1, semantic: difft),
        ChangedFile(path: 'n.txt', status: 'A', added: 1, removed: 0),
      ],
      diff: unified,
    );
    final semantic = diffViewLines(changes, semantic: true);
    expect(semantic.where((l) => l.file == 'a.rs').any((l) => l.kind == DiffLineKind.hunk), isFalse);
    expect(semantic.where((l) => l.file == 'n.txt').any((l) => l.kind == DiffLineKind.hunk), isTrue, reason: 'no semantic: text fallback');
    final text = diffViewLines(changes, semantic: false);
    expect(text.where((l) => l.file == 'a.rs').any((l) => l.kind == DiffLineKind.hunk), isTrue);
  });
}
