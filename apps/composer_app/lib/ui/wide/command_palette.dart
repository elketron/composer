import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../state/actions.dart';
import '../../state/types.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../chat/navigation.dart';
import '../chat/project_sheet.dart';
import '../chat/review_page.dart';
import '../home/activity_dot.dart';
import '../home/new_chat_sheet.dart';
import '../pages/page_navigation.dart';
import '../store_scope.dart';

/// Opens the palette (ctrl-k) for the active server.
void showPalette(BuildContext context, String url) => showDialog<void>(
      context: context,
      builder: (_) => Dialog(
        alignment: const Alignment(0, -0.7),
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 600, maxHeight: 480),
          child: CommandPalette(url: url, parent: context),
        ),
      ),
    );

/// One thing the palette offers.
class _Entry {
  const _Entry(this.group, this.label, this.run, {this.detail, this.hint, this.activity});
  final String group;
  final String label;
  final String? detail;

  /// A shortcut, or where it is.
  final String? hint;

  /// A session's state, for its dot.
  final String? activity;
  final VoidCallback run;
}

/// One box for sessions, pages and actions: type to filter, arrows to choose, enter to go.
class CommandPalette extends StatefulWidget {
  const CommandPalette({super.key, required this.url, required this.parent});

  final String url;

  /// Where actions run (the dialog's own context goes away with it).
  final BuildContext parent;

  @override
  State<CommandPalette> createState() => _CommandPaletteState();
}

class _CommandPaletteState extends State<CommandPalette> {
  final _query = TextEditingController();
  int _selected = 0;

  @override
  void dispose() {
    _query.dispose();
    super.dispose();
  }

  List<_Entry> _entries() {
    final parent = widget.parent;
    final store = StoreScope.read(parent);
    final state = store.state;
    final url = widget.url;
    final status = state.statusOf(url);
    final chat = status.chat;
    final session = chat == null ? null : status.session(chat.session);
    final q = _query.text.trim().toLowerCase();
    bool matches(String text) => q.isEmpty || q.split(' ').every((word) => text.toLowerCase().contains(word));

    final actions = <_Entry>[
      _Entry('Actions', 'New chat', () => showNewChat(parent, url), hint: 'ctrl n'),
      if (session != null) ...[
        _Entry('Actions', session.project == null ? 'Set the project…' : 'Change the project…', () => showProjectSheet(parent, url),
            detail: session.project?.split('/').last),
        _Entry('Actions', 'Compact this chat', () => store.dispatch(CompactRequested(url)), detail: 'save memory, summarize'),
        if (session.state != TurnState.idle) _Entry('Actions', 'Stop the turn', () => store.dispatch(CancelRequested(url))),
      ],
      for (final w in status.sessions.where((s) => s.ticket != null && (status.workers[s.id] ?? s.worker) == WorkerState.done))
        _Entry('Actions', 'Review changes of ticket ${w.ticket}', () => openReview(parent, url, w.ticket!), detail: w.preview, hint: 'merge · discard'),
      _Entry('Actions', 'Open the reader', () => openPage(parent, url, status.page.page?.url ?? 'site://home'), hint: 'ctrl p'),
      _Entry('Actions', state.layout.sessionsCollapsed ? 'Show the sessions' : 'Fold the sessions', () => store.dispatch(const SessionsToggled()),
          hint: 'ctrl b'),
      _Entry('Actions', 'This server', () => store.dispatch(SideTabSelected(url, SideTab.server))),
      for (final site in status.sites.where((s) => s.appBuilt))
        _Entry('Actions', 'Open the ${site.name} app', () => openApp(parent, url, site.name), detail: site.description),
      for (final (i, other) in state.servers.indexed)
        if (other.url != url) _Entry('Actions', 'Switch to ${other.name}', () => store.dispatch(ServerSelected(other.url)), hint: i < 9 ? 'ctrl ${i + 1}' : null),
    ];
    final sessions = [
      for (final s in status.sessions)
        _Entry(
          'Sessions',
          '${s.agent} #${s.id}',
          () => openSession(parent, url, s.id),
          detail: [if (s.ticket != null) 'ticket ${s.ticket}', s.preview ?? '', if (s.project != null) s.project!.split('/').last].join(' · '),
          activity: status.activity(s),
        ),
    ];
    final pages = q.isEmpty || status.search.query != _query.text
        ? const <_Entry>[]
        : [
            for (final hit in status.search.hits ?? const <SearchHit>[])
              _Entry('Pages', hit.title, () => openPage(parent, url, hit.url), detail: hit.url.replaceFirst('site://', ''), hint: hit.site),
          ];
    return [
      ...actions.where((e) => matches('${e.label} ${e.detail ?? ''}')),
      ...sessions.where((e) => matches('${e.label} ${e.detail ?? ''}')).take(q.isEmpty ? 6 : 12),
      ...pages.take(8),
    ];
  }

  void _run(_Entry entry) {
    Navigator.pop(context);
    entry.run();
  }

  @override
  Widget build(BuildContext context) {
    // Rebuilt as search results arrive.
    StoreScope.of(context);
    final entries = _entries();
    final selected = entries.isEmpty ? 0 : _selected.clamp(0, entries.length - 1);
    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.arrowDown): () => setState(() => _selected = (selected + 1) % entries.length.clamp(1, 1 << 30)),
        const SingleActivator(LogicalKeyboardKey.arrowUp): () =>
            setState(() => _selected = (selected - 1 + entries.length) % entries.length.clamp(1, 1 << 30)),
        const SingleActivator(LogicalKeyboardKey.enter): () {
          if (entries.isNotEmpty) _run(entries[selected]);
        },
      },
      child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        TextField(
          key: const Key('palette-query'),
          controller: _query,
          autofocus: true,
          decoration: const InputDecoration(
            hintText: 'Sessions, pages and actions',
            prefixIcon: Icon(Icons.search, size: 18, color: t.faint),
            border: InputBorder.none,
            enabledBorder: InputBorder.none,
            focusedBorder: InputBorder.none,
          ),
          onChanged: (query) {
            setState(() => _selected = 0);
            StoreScope.read(context).dispatch(SearchChanged(widget.url, query));
          },
        ),
        const Divider(height: 1),
        Flexible(
          child: entries.isEmpty
              ? const Padding(padding: EdgeInsets.all(16), child: Text('Nothing matches.', style: TextStyle(color: t.faint)))
              : ListView.builder(
                  shrinkWrap: true,
                  padding: const EdgeInsets.symmetric(vertical: 4),
                  itemCount: entries.length,
                  itemBuilder: (_, i) {
                    final e = entries[i];
                    final header = i == 0 || entries[i - 1].group != e.group;
                    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                      if (header)
                        Padding(
                          padding: const EdgeInsets.fromLTRB(14, 8, 14, 3),
                          child: Text(e.group.toUpperCase(), style: mono(size: 10, color: t.faint)),
                        ),
                      InkWell(
                        key: Key('palette-$i'),
                        onTap: () => _run(e),
                        child: Container(
                          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 6),
                          decoration: i == selected
                              ? BoxDecoration(
                                  color: t.accent.withValues(alpha: 0.12),
                                  border: const Border(left: BorderSide(color: t.accent, width: 2)),
                                )
                              : null,
                          child: Row(children: [
                            if (e.activity != null) ...[ActivityDot(e.activity!, size: 6), const SizedBox(width: 8)],
                            Text(e.label, style: e.group == 'Actions' ? const TextStyle(fontSize: 13) : mono(size: 12, color: t.text)),
                            if (e.detail != null && e.detail!.isNotEmpty) ...[
                              const SizedBox(width: 10),
                              Expanded(
                                child: Text(e.detail!, maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 12, color: t.muted)),
                              ),
                            ] else
                              const Spacer(),
                            if (e.hint != null) ...[const SizedBox(width: 10), Text(e.hint!, style: mono(size: 11, color: t.faint))],
                          ]),
                        ),
                      ),
                    ]);
                  },
                ),
        ),
        const Divider(height: 1),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 7),
          child: Text('↑↓ choose · enter go · esc close', style: mono(size: 10.5, color: t.faint)),
        ),
      ]),
    );
  }
}
