import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/run_kind.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../chat/navigation.dart';
import '../store_scope.dart';

/// Opens New chat for a server, as a sheet.
///
/// With [site], that site is preselected.
void showNewChat(BuildContext context, String url, {String? site}) => showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      showDragHandle: true,
      builder: (_) => NewChatSheet(url: url, parent: context, site: site),
    );

/// A new chat: its run kind and, optionally, the site it starts in. It starts agentic, so Start is
/// one tap.
class NewChatSheet extends StatefulWidget {
  const NewChatSheet({super.key, required this.url, required this.parent, this.site});

  final String url;

  /// The site the chat starts in, preselected.
  final String? site;

  /// Where the chat opens (the sheet's own context goes away with it).
  final BuildContext parent;

  @override
  State<NewChatSheet> createState() => _NewChatSheetState();
}

class _NewChatSheetState extends State<NewChatSheet> {
  RunKind _kind = RunKind.agentic;
  late String? _site = widget.site;

  static const _kinds = [
    (RunKind.agentic, 'a conversation with every tool'),
    (RunKind.workflow, 'read-only tools; ends with a result'),
    (RunKind.singleShot, 'one answer, no tools'),
  ];

  @override
  Widget build(BuildContext context) {
    final sites = StoreScope.of(context).state.statusOf(widget.url).sites;

    void start() {
      Navigator.pop(context);
      StoreScope.read(widget.parent).dispatch(NewSessionRequested(widget.url, kind: _kind, site: _site));
      pushChat(widget.parent, widget.url);
    }

    Widget choice(RunKind kind, String description) {
      final on = kind == _kind;
      final name = runKindLabel(kind);
      return Padding(
        padding: const EdgeInsets.only(bottom: 7),
        child: Material(
          color: on ? t.accent.withValues(alpha: 0.12) : Colors.transparent,
          shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(t.radius),
            side: BorderSide(color: on ? t.accent.withValues(alpha: 0.55) : t.border),
          ),
          child: InkWell(
            key: Key('kind-${kind.wire}'),
            borderRadius: BorderRadius.circular(t.radius),
            onTap: () => setState(() => _kind = kind),
            child: Padding(
              padding: const EdgeInsets.all(11),
              child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
                Icon(on ? Icons.radio_button_checked : Icons.radio_button_off, size: 16, color: on ? t.accent : t.borderStrong),
                const SizedBox(width: 10),
                Expanded(
                  child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                    Text(name, style: mono(size: 13, color: t.text, weight: FontWeight.w500)),
                    Text(description, style: const TextStyle(fontSize: 12, color: t.muted)),
                  ]),
                ),
              ]),
            ),
          ),
        ),
      );
    }

    Widget siteChip(String? site) => ChoiceChip(
          key: Key('new-chat-site-${site ?? 'none'}'),
          label: Text(site ?? 'no site', style: mono(size: 12)),
          selected: site == _site,
          onSelected: (_) => setState(() => _site = site),
        );

    final names = {for (final s in sites) s.name, ?widget.site};
    return SafeArea(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
        child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          const Text('New chat', style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600)),
          const SizedBox(height: 12),
          Text('KIND', style: mono(size: 10.5, color: t.faint)),
          const SizedBox(height: 8),
          for (final (kind, description) in _kinds) choice(kind, description),
          const SizedBox(height: 10),
          Text('SITE', style: mono(size: 10.5, color: t.faint)),
          const SizedBox(height: 8),
          ConstrainedBox(
            constraints: const BoxConstraints(maxHeight: 160),
            child: SingleChildScrollView(
              child: Wrap(spacing: 6, runSpacing: 6, children: [siteChip(null), for (final name in names) siteChip(name)]),
            ),
          ),
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
