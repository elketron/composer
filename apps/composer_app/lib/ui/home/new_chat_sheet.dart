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

  /// The outside agent that runs it; null: composer's own. Agentic only.
  String? _harness;

  final _scroll = ScrollController();

  @override
  void dispose() {
    _scroll.dispose();
    super.dispose();
  }

  static const _kinds = [
    (RunKind.agentic, 'a conversation with every tool'),
    (RunKind.workflow, 'read-only tools; ends with a result'),
    (RunKind.singleShot, 'one answer, no tools'),
  ];

  @override
  Widget build(BuildContext context) {
    final status = StoreScope.of(context).state.statusOf(widget.url);
    final sites = status.sites;
    final harnesses = status.info?.harnesses ?? const <String>[];

    void start() {
      Navigator.pop(context);
      StoreScope.read(widget.parent).dispatch(NewSessionRequested(widget.url, kind: _kind, site: _site, harness: _harness));
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

    // The selected state the kind rows use: a faint accent fill and edge, the label in full text colour.
    Widget chip({required Key key, required String label, required bool selected, required VoidCallback onSelected}) => ChoiceChip(
          key: key,
          label: Text(label, style: mono(size: 12, color: selected ? t.text : t.muted)),
          selected: selected,
          selectedColor: t.accent.withValues(alpha: 0.12),
          backgroundColor: Colors.transparent,
          checkmarkColor: t.accent,
          side: BorderSide(color: selected ? t.accent.withValues(alpha: 0.55) : t.border),
          onSelected: (_) => onSelected(),
        );

    Widget siteChip(String? site) => chip(
          key: Key('new-chat-site-${site ?? 'none'}'),
          label: site ?? 'no site',
          selected: site == _site,
          onSelected: () => setState(() => _site = site),
        );

    Widget harnessChip(String? harness) => chip(
          key: Key('new-chat-harness-${harness ?? 'composer'}'),
          label: harness ?? 'composer',
          selected: harness == _harness,
          onSelected: () => setState(() => _harness = harness),
        );

    final names = {for (final s in sites) s.name, ?widget.site};
    // Everything above Start scrolls when it does not fit (a server can have many sites), so every
    // site is reachable and Start stays in view.
    return SafeArea(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
        child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Flexible(
            child: Scrollbar(
              controller: _scroll,
              thumbVisibility: true,
              child: SingleChildScrollView(
                key: const Key('new-chat-scroll'),
                controller: _scroll,
                child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                  const Text('New chat', style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600)),
                  const SizedBox(height: 12),
                  Text('KIND', style: mono(size: 10.5, color: t.faint)),
                  const SizedBox(height: 8),
                  for (final (kind, description) in _kinds) choice(kind, description),
                  if (_kind == RunKind.agentic && harnesses.isNotEmpty) ...[
                    const SizedBox(height: 10),
                    Text('AGENT', style: mono(size: 10.5, color: t.faint)),
                    const SizedBox(height: 8),
                    Wrap(spacing: 6, runSpacing: 6, children: [harnessChip(null), for (final h in harnesses) harnessChip(h)]),
                  ],
                  const SizedBox(height: 10),
                  Text('SITE', style: mono(size: 10.5, color: t.faint)),
                  const SizedBox(height: 8),
                  Wrap(spacing: 6, runSpacing: 6, children: [siteChip(null), for (final name in names) siteChip(name)]),
                ]),
              ),
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
