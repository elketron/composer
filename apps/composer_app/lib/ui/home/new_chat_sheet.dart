import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../chat/navigation.dart';
import '../store_scope.dart';

/// Opens New chat for a server, as a sheet.
///
/// With [site], the new chat enters that site once it exists.
void showNewChat(BuildContext context, String url, {String? site}) => showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      showDragHandle: true,
      builder: (_) => NewChatSheet(url: url, parent: context, site: site),
    );

/// The agent for a new chat; it starts on `default`, so Start is one tap.
// rewrite: phase 2 — the sheet picks a run kind instead.
class NewChatSheet extends StatefulWidget {
  const NewChatSheet({super.key, required this.url, required this.parent, this.site});

  final String url;

  /// The site the chat starts in.
  final String? site;

  /// Where the chat opens (the sheet's own context goes away with it).
  final BuildContext parent;

  @override
  State<NewChatSheet> createState() => _NewChatSheetState();
}

class _NewChatSheetState extends State<NewChatSheet> {
  String _agent = 'default';

  @override
  Widget build(BuildContext context) {
    final status = StoreScope.of(context).state.statusOf(widget.url);
    final agents = status.agents;

    void start() {
      Navigator.pop(context);
      StoreScope.read(widget.parent).dispatch(NewSessionRequested(widget.url, agent: _agent, site: widget.site));
      pushChat(widget.parent, widget.url);
    }

    Widget choice(String name, String description) {
      final on = name == _agent;
      return Padding(
        padding: const EdgeInsets.only(bottom: 7),
        child: Material(
          color: on ? t.accent.withValues(alpha: 0.12) : Colors.transparent,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(t.radius),
            side: BorderSide(color: on ? t.accent.withValues(alpha: 0.55) : t.border),
          ),
          child: InkWell(
            key: Key('agent-$name'),
            borderRadius: BorderRadius.circular(t.radius),
            onTap: () => setState(() => _agent = name),
            child: Padding(
              padding: const EdgeInsets.all(11),
              child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
                Icon(on ? Icons.radio_button_checked : Icons.radio_button_off, size: 16, color: on ? t.accent : t.borderStrong),
                const SizedBox(width: 10),
                Expanded(
                  child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                    Text(name, style: mono(size: 13, color: t.text, weight: FontWeight.w500)),
                    if (description.isNotEmpty && description.toLowerCase() != name)
                      Text(description, style: const TextStyle(fontSize: 12, color: t.muted)),
                  ]),
                ),
              ]),
            ),
          ),
        ),
      );
    }

    return SafeArea(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
        child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Text(widget.site == null ? 'New chat' : 'New chat in ${widget.site}', style: const TextStyle(fontSize: 16, fontWeight: FontWeight.w600)),
          const SizedBox(height: 12),
          Text('AGENT', style: mono(size: 10.5, color: t.faint)),
          const SizedBox(height: 8),
          if (agents.isEmpty) choice('default', 'the agent you talk to'),
          for (final a in agents) choice(a.name, a.description),
          const SizedBox(height: 18),
          FilledButton(
            key: const Key('start-chat'),
            style: FilledButton.styleFrom(minimumSize: const Size.fromHeight(44)),
            onPressed: start,
            child: const Text('Start chat'),
          ),
        ]),
      ),
    );
  }
}
