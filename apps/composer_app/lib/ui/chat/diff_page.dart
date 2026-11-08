import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/diff_lines.dart';
import '../../state/types.dart';
import '../../state/workspace.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';
import '../workspace_mark.dart';
import 'diff_comment_box.dart';
import 'review_actions.dart';

/// Opens a ticket's diff full screen (a pushed route, on phones and wide screens alike).
void openDiff(BuildContext context, String url, String ticket) {
  Navigator.of(context).push(MaterialPageRoute<void>(builder: (_) => DiffPage(url: url, ticket: ticket)));
}

/// A ticket's diff in one column: the structural diff where the server sent one (else the text
/// diff), every line tappable to comment on, the comments below, Send, Merge and Discard.
class DiffPage extends StatefulWidget {
  const DiffPage({super.key, required this.url, required this.ticket});

  final String url;
  final String ticket;

  @override
  State<DiffPage> createState() => _DiffPageState();
}

class _DiffPageState extends State<DiffPage> {
  bool _semantic = true;

  /// The line whose comment box is open, and the draft being edited instead of added.
  int? _open;
  int? _editing;

  /// A long press marked the start of a range here; the next tap picks its end.
  int? _from;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.of(context);
    final (url, ticket) = (widget.url, widget.ticket);
    final review = store.state.statusOf(url).reviews[ticket] ?? const TicketReviewState(busy: true);
    final changes = review.changes;
    final hasSemantic = changes?.files.any((f) => f.semantic != null) ?? false;
    final lines = changes == null ? const <DiffLine>[] : diffViewLines(changes, semantic: _semantic && hasSemantic);
    final closed = review.merged != null || review.discarded;
    final workspace = Workspace.of(url, info: store.state.statusOf(url).info, entry: store.state.serverFor(url));
    return Scaffold(
      appBar: AppBar(
        titleSpacing: 0,
        title: Text('Diff · ticket $ticket', style: mono(size: 15, color: t.text, weight: FontWeight.w600)),
        actions: [
          if (hasSemantic)
            Padding(
              padding: const EdgeInsets.only(right: 8),
              child: SegmentedButton<bool>(
                key: const Key('diff-mode'),
                showSelectedIcon: false,
                segments: const [
                  ButtonSegment(value: true, label: Text('Semantic')),
                  ButtonSegment(value: false, label: Text('Text')),
                ],
                selected: {_semantic},
                onSelectionChanged: (v) => setState(() {
                  _semantic = v.first;
                  _open = null;
                  _editing = null;
                  _from = null;
                }),
              ),
            ),
        ],
        bottom: PreferredSize(preferredSize: const Size.fromHeight(2), child: WorkspaceStrip(workspace: workspace)),
      ),
      body: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        if (review.busy) const LinearProgressIndicator(minHeight: 2),
        if (review.error != null) _banner(review.error!, t.err),
        if (closed) _banner(review.merged != null ? 'Merged as ${review.merged}.' : 'Discarded.', t.ok),
        Expanded(
          child: ListView(padding: const EdgeInsets.fromLTRB(12, 8, 12, 12), children: [
            if (changes != null && lines.isEmpty) const Text('No changes on its branch yet.', style: TextStyle(color: t.faint, fontSize: 13)),
            for (var i = 0; i < lines.length; i++) ..._line(store, review, lines, i),
            const SizedBox(height: 16),
            ..._comments(store, review),
          ]),
        ),
        if (review.drafts.isNotEmpty && !closed)
          Container(
            padding: const EdgeInsets.fromLTRB(12, 8, 12, 8),
            decoration: const BoxDecoration(border: Border(top: BorderSide(color: t.border))),
            child: FilledButton(
              key: const Key('send-comments'),
              onPressed: review.busy ? null : () => store.dispatch(DiffCommentsSent(url, ticket)),
              child: Text('Send ${review.drafts.length} comment${review.drafts.length == 1 ? '' : 's'}'),
            ),
          ),
        ReviewActions(url: url, ticket: ticket),
      ]),
    );
  }

  List<Widget> _line(AppStore store, TicketReviewState review, List<DiffLine> lines, int index) {
    final line = lines[index];
    if (line.kind == DiffLineKind.file) {
      final file = review.changes?.files.where((f) => f.path == line.file).firstOrNull;
      return [
        Container(
          margin: const EdgeInsets.only(top: 10),
          padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 4),
          color: t.raised,
          child: Row(children: [
            Expanded(child: Text(line.file, style: mono(size: 12, color: t.text, weight: FontWeight.w600))),
            if (file != null) Text('+${file.added} -${file.removed}', style: mono(size: 11.5, color: t.muted)),
          ]),
        ),
      ];
    }
    final (color, background) = switch (line.kind) {
      DiffLineKind.hunk => (t.info, null),
      DiffLineKind.add => (t.ok, t.ok.withValues(alpha: 0.06)),
      DiffLineKind.del => (t.err, t.err.withValues(alpha: 0.06)),
      DiffLineKind.meta => (t.faint, null),
      _ => (t.muted, null),
    };
    final marked = review.drafts.any((d) => d.file == line.file && line.line != null && d.start <= line.line! && line.line! <= d.end);
    final text = Container(
      color: _from == index ? t.accent.withValues(alpha: 0.15) : background,
      width: double.infinity,
      child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Expanded(child: Text(line.text.isEmpty ? ' ' : line.text, style: mono(size: 11.5, color: color))),
        if (marked) const Icon(Icons.comment, size: 12, color: t.accent),
      ]),
    );
    return [
      if (line.commentable)
        InkWell(
          key: Key('diff-line-$index'),
          onLongPress: () => setState(() => _from = _from == index ? null : index),
          onTap: () => setState(() {
            _open = _open == index ? null : index;
            _editing = null;
          }),
          child: text,
        )
      else
        text,
      if (_open == index && _editing == null) _newComment(store, lines, index),
    ];
  }

  /// The box under the tapped line; with a range marked (long press) it comments on the span.
  Widget _newComment(AppStore store, List<DiffLine> lines, int index) {
    final from = _from ?? index;
    final preview = diffAnchor(lines, from, index, '');
    if (preview == null) {
      return DiffCommentBox(anchor: 'No single file here: pick lines of one file', onCancel: _close, onSave: (_) {});
    }
    return DiffCommentBox(
      anchor: '${preview.file}:${preview.start}:${preview.end}',
      quote: preview.quote,
      onCancel: _close,
      onSave: (message) {
        store.dispatch(DiffCommentAdded(widget.url, widget.ticket, diffAnchor(lines, from, index, message)!));
        _close();
      },
    );
  }

  void _close() => setState(() {
        _open = null;
        _from = null;
      });

  List<Widget> _comments(AppStore store, TicketReviewState review) {
    if (review.drafts.isEmpty && review.comments.isEmpty) return const [];
    return [
      if (review.drafts.isNotEmpty) ...[
        Text('Comments (${review.drafts.length}, not sent)', style: const TextStyle(fontSize: 13, color: t.muted)),
        for (var i = 0; i < review.drafts.length; i++)
          if (_editing == i)
            DiffCommentBox(
              anchor: _anchor(review.drafts[i]),
              quote: review.drafts[i].quote,
              initial: review.drafts[i].message,
              onCancel: () => setState(() => _editing = null),
              onSave: (message) {
                store.dispatch(DiffCommentEdited(widget.url, widget.ticket, i, message));
                setState(() => _editing = null);
              },
            )
          else
            Row(key: Key('draft-$i'), children: [
              Expanded(child: Text('${_anchor(review.drafts[i])} — ${review.drafts[i].message}', style: mono(size: 12, color: t.text))),
              IconButton(
                key: Key('draft-edit-$i'),
                tooltip: 'Edit',
                icon: const Icon(Icons.edit, size: 18, color: t.muted),
                onPressed: () => setState(() {
                  _editing = i;
                  _open = null;
                }),
              ),
              IconButton(
                key: Key('draft-remove-$i'),
                tooltip: 'Remove',
                icon: const Icon(Icons.delete_outline, size: 18, color: t.muted),
                onPressed: () => store.dispatch(DiffCommentRemoved(widget.url, widget.ticket, i)),
              ),
            ]),
        const SizedBox(height: 12),
      ],
      if (review.comments.isNotEmpty) ...[
        const Text('On the ticket', style: TextStyle(fontSize: 13, color: t.muted)),
        for (final line in review.comments) Text(line, style: mono(size: 12, color: t.faint)),
      ],
    ];
  }

  String _anchor(DiffComment c) => '${c.file}:${c.start}:${c.end}';

  Widget _banner(String text, Color color) => Container(
        margin: const EdgeInsets.fromLTRB(12, 8, 12, 0),
        padding: const EdgeInsets.all(10),
        decoration: BoxDecoration(color: color.withValues(alpha: 0.06), border: Border.all(color: color.withValues(alpha: 0.4))),
        child: Text(text, style: TextStyle(fontSize: 13, color: color)),
      );
}
