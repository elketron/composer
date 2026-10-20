import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../state/actions.dart';
import '../../theme/tokens.dart' as t;
import '../../state/types.dart';
import '../store_scope.dart';
import 'attach_sheet.dart';
import 'square_button.dart';
import 'upload_chips.dart';

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
  bool _drafting = false;

  @override
  void initState() {
    super.initState();
    _text.addListener(() {
      setState(() {});
      // Only the change matters (a worker finishing doesn't switch the chat away from a draft).
      final drafting = _text.text.trim().isNotEmpty;
      if (drafting != _drafting) {
        _drafting = drafting;
        StoreScope.read(context).dispatch(DraftChanged(widget.url, drafting: drafting));
      }
    });
    if (Platform.isLinux || Platform.isWindows || Platform.isMacOS) _focus.requestFocus();
  }

  @override
  void dispose() {
    _text.dispose();
    _focus.dispose();
    super.dispose();
  }

  /// Something to send (text or attached files), and every file uploaded.
  bool _canSend(List<Upload> uploads) =>
      (_text.text.trim().isNotEmpty || uploads.isNotEmpty) && uploads.every((u) => u.status == UploadStatus.ready);

  void _send() {
    final store = StoreScope.read(context);
    final uploads = store.state.statusOf(widget.url).chat?.uploads ?? const <Upload>[];
    if (!_canSend(uploads)) return;
    store.dispatch(MessageSubmitted(widget.url, _text.text.trim()));
    _text.clear();
  }

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.of(context);
    final uploads = store.state.statusOf(widget.url).chat?.uploads ?? const <Upload>[];
    final idle = _text.text.trim().isEmpty && uploads.isEmpty;
    return Container(
      padding: const EdgeInsets.all(10),
      decoration: const BoxDecoration(
        color: t.surface,
        border: Border(top: BorderSide(color: t.border)),
      ),
      child: SafeArea(
        top: false,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            if (uploads.isNotEmpty) UploadChips(url: widget.url, uploads: uploads),
            Row(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                SquareButton(
                  key: const Key('attach'),
                  tooltip: 'Attach a photo or file',
                  color: t.raised,
                  onPressed: () => showAttachSheet(context, widget.url),
                  child: const Icon(Icons.attach_file, size: 18, color: t.muted),
                ),
                const SizedBox(width: 8),
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
                        hintText: widget.busy ? 'Message (sent after this turn)' : 'Message',
                        contentPadding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
                      ),
                    ),
                  ),
                ),
                // Writing while the agent works: Stop stays beside Send (the message waits for the turn).
                if (widget.busy && !idle) ...[
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
                // Nothing written while it works: Stop in Send's place.
                if (widget.busy && idle)
                  SquareButton(
                    key: const Key('cancel'),
                    tooltip: 'Stop the turn',
                    color: t.raised,
                    child: const Icon(Icons.stop, size: 18, color: t.err),
                    onPressed: () => store.dispatch(CancelRequested(widget.url)),
                  )
                else
                  SquareButton(
                    key: const Key('send'),
                    tooltip: 'Send (ctrl-enter)',
                    color: t.accent,
                    onPressed: _canSend(uploads) ? _send : null,
                    child: const Icon(Icons.arrow_upward, size: 18, color: t.bg),
                  ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
