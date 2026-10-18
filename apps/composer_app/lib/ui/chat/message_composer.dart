import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../state/actions.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';
import 'square_button.dart';

/// Writing a message. Enter is a newline; ctrl-enter (or the button) sends. While the agent
/// works, the message waits for the turn to end, and stop cancels the turn.
class MessageComposer extends StatefulWidget {
  const MessageComposer({super.key, required this.url, required this.busy});

  final String url;
  final bool busy;

  @override
  State<MessageComposer> createState() => _MessageComposerState();
}

class _MessageComposerState extends State<MessageComposer> {
  final _text = TextEditingController();
  final _focus = FocusNode();

  @override
  void initState() {
    super.initState();
    _text.addListener(() => setState(() {}));
    if (Platform.isLinux || Platform.isWindows || Platform.isMacOS) _focus.requestFocus();
  }

  @override
  void dispose() {
    _text.dispose();
    _focus.dispose();
    super.dispose();
  }

  void _send() {
    final text = _text.text.trim();
    if (text.isEmpty) return;
    StoreScope.read(context).dispatch(MessageSubmitted(widget.url, text));
    _text.clear();
  }

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.read(context);
    return Container(
      padding: const EdgeInsets.all(10),
      decoration: const BoxDecoration(color: t.surface, border: Border(top: BorderSide(color: t.border))),
      child: SafeArea(
        top: false,
        child: Row(crossAxisAlignment: CrossAxisAlignment.end, children: [
          Expanded(
            child: CallbackShortcuts(
              bindings: {
                const SingleActivator(LogicalKeyboardKey.enter, control: true): _send,
                const SingleActivator(LogicalKeyboardKey.enter, meta: true): _send,
              },
              child: TextField(
                key: const Key('message'),
                controller: _text,
                focusNode: _focus,
                minLines: 1,
                maxLines: 8,
                keyboardType: TextInputType.multiline,
                textCapitalization: TextCapitalization.sentences,
                style: const TextStyle(fontSize: 15, height: 1.4),
                decoration: InputDecoration(
                  hintText: widget.busy ? 'Message (after this turn)' : 'Message',
                  contentPadding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
                ),
              ),
            ),
          ),
          if (widget.busy) ...[
            const SizedBox(width: 8),
            SquareButton(
              key: const Key('cancel'),
              tooltip: 'Stop the turn',
              color: t.raised,
              child: const Icon(Icons.stop, size: 18, color: t.err),
              onPressed: () => store.dispatch(CancelRequested(widget.url)),
            ),
          ],
          const SizedBox(width: 8),
          SquareButton(
            key: const Key('send'),
            tooltip: 'Send (ctrl-enter)',
            color: t.accent,
            onPressed: _text.text.trim().isEmpty ? null : _send,
            child: const Icon(Icons.arrow_upward, size: 18, color: t.bg),
          ),
        ]),
      ),
    );
  }
}
