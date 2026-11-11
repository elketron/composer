import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../chat/navigation.dart';
import '../store_scope.dart';

/// Opens New chat for a server, as a sheet.
///
/// With [site], the new chat enters that workflow once it exists.
void showNewChat(BuildContext context, String url, {String? site}) => showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      showDragHandle: true,
      builder: (_) => NewChatSheet(url: url, parent: context, site: site),
    );

/// Agent and project for a new chat; both start on a sensible choice, so Start is one tap.
class NewChatSheet extends StatefulWidget {
  const NewChatSheet({super.key, required this.url, required this.parent, this.site});

  final String url;

  /// The workflow the chat starts in.
  final String? site;

  /// Where the chat opens (the sheet's own context goes away with it).
  final BuildContext parent;

  @override
  State<NewChatSheet> createState() => _NewChatSheetState();
}

class _NewChatSheetState extends State<NewChatSheet> {
  String _agent = 'default';
  String? _project;
  bool _picked = false;

  @override
  Widget build(BuildContext context) {
    final status = StoreScope.of(context).state.statusOf(widget.url);
    final agents = status.agents.where((a) => !a.worker).toList();
    final projects = status.projects.take(8).map((p) => p.name).toList();
    // The project you are in now (a worker's: its orchestrator's, not its worktree), else the
    // most recently changed one.
    final open = status.chat == null ? null : status.session(status.chat!.session);
    final home = open?.ticket != null && open?.parent != null ? status.session(open!.parent!) : open;
    // By its full path: it needn't be in the projects folder.
    final current = home?.project;
    final project = _picked ? _project : (current ?? projects.firstOrNull);

    void start(String? project) {
      Navigator.pop(context);
      StoreScope.read(widget.parent).dispatch(NewSessionRequested(widget.url, agent: _agent, project: project, site: widget.site));
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

    /// `name` is a folder in the projects directory, or a path.
    Widget projectChip(String? name) {
      final on = name == project;
      final label = name?.split('/').last ?? 'none';
      return ChoiceChip(
        key: Key('project-$label'),
        label: Text(label, style: mono(size: 12, color: on ? t.text : t.muted)),
        selected: on,
        showCheckmark: false,
        selectedColor: t.accent.withValues(alpha: 0.12),
        backgroundColor: t.surface,
        side: BorderSide(color: on ? t.accent : t.borderStrong),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(t.radius)),
        onSelected: (_) => setState(() {
          _picked = true;
          _project = name;
        }),
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
          if (agents.isEmpty) choice('default', 'general assistant'),
          for (final a in agents) choice(a.name, a.description),
          const SizedBox(height: 10),
          Text('PROJECT', style: mono(size: 10.5, color: t.faint)),
          const SizedBox(height: 8),
          Wrap(spacing: 6, runSpacing: 6, children: [
            ?current == null ? null : projectChip(current),
            for (final p in projects)
              if (p != current?.split('/').last) projectChip(p),
            projectChip(null),
          ]),
          if ((status.info?.projectsDir ?? '').isNotEmpty) ...[
            const SizedBox(height: 6),
            Text('Folders in ${status.info!.projectsDir}', style: const TextStyle(fontSize: 12, color: t.faint)),
          ],
          const SizedBox(height: 18),
          FilledButton(
            key: const Key('start-chat'),
            style: FilledButton.styleFrom(minimumSize: const Size.fromHeight(44)),
            onPressed: () => start(project),
            child: const Text('Start chat'),
          ),
        ]),
      ),
    );
  }
}
