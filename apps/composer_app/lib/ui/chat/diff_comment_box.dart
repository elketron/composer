import 'package:flutter/material.dart';

import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;

/// The box under a tapped diff line: the anchor, the quoted line, a field, Add.
class DiffCommentBox extends StatefulWidget {
  const DiffCommentBox({super.key, required this.anchor, this.quote, this.initial = '', required this.onSave, required this.onCancel});

  final String anchor;
  final String? quote;
  final String initial;
  final ValueChanged<String> onSave;
  final VoidCallback onCancel;

  @override
  State<DiffCommentBox> createState() => _DiffCommentBoxState();
}

class _DiffCommentBoxState extends State<DiffCommentBox> {
  late final _controller = TextEditingController(text: widget.initial);

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _save() {
    final text = _controller.text.trim();
    if (text.isNotEmpty) widget.onSave(text);
  }

  @override
  Widget build(BuildContext context) => Container(
        margin: const EdgeInsets.symmetric(vertical: 4),
        padding: const EdgeInsets.all(8),
        decoration: BoxDecoration(color: t.raised, border: Border.all(color: t.border)),
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Text(widget.anchor, style: mono(size: 12, color: t.accent)),
          if (widget.quote != null) Text(widget.quote!, maxLines: 2, overflow: TextOverflow.ellipsis, style: mono(size: 11.5, color: t.muted)),
          TextField(
            key: const Key('comment-field'),
            controller: _controller,
            autofocus: true,
            minLines: 1,
            maxLines: 4,
            decoration: const InputDecoration(hintText: 'Comment', isDense: true),
          ),
          const SizedBox(height: 6),
          Row(mainAxisAlignment: MainAxisAlignment.end, children: [
            TextButton(onPressed: widget.onCancel, child: const Text('Cancel')),
            FilledButton(key: const Key('comment-add'), onPressed: _save, child: const Text('Save')),
          ]),
        ]),
      );
}
