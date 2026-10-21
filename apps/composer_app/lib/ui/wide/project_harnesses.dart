import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/server_status.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../chat/chat_menu.dart';
import '../home/section_label.dart';
import '../store_scope.dart';

/// Per project, the external harnesses config.toml allows there, each with its on / off switch
/// (`harness.set`). Projects that allow none aren't listed.
class ProjectHarnesses extends StatefulWidget {
  const ProjectHarnesses({super.key, required this.url, required this.status});

  final String url;
  final ServerStatus status;

  @override
  State<ProjectHarnesses> createState() => _ProjectHarnessesState();
}

class _ProjectHarnessesState extends State<ProjectHarnesses> {
  @override
  void initState() {
    super.initState();
    // What each project allows, asked once when shown (after the frame: not while building).
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      final store = StoreScope.read(context);
      for (final project in widget.status.projects) {
        store.dispatch(HarnessesRequested(widget.url, project.folder ?? project.path));
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.read(context);
    final projects = [
      for (final MapEntry(key: folder, value: switches) in widget.status.harnesses.entries)
        if (switches.any((h) => h.allowed && !h.entry.contains(':'))) (folder, switches),
    ]..sort((a, b) => a.$1.compareTo(b.$1));
    if (projects.isEmpty) return const SizedBox.shrink();
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      const SectionLabel('Projects · external harnesses'),
      Container(
        decoration: BoxDecoration(color: t.surface, border: Border.all(color: t.border), borderRadius: BorderRadius.circular(t.radius)),
        child: Column(children: [
          for (final (i, (folder, switches)) in projects.indexed) ...[
            if (i > 0) const Divider(height: 1),
            Padding(
              padding: const EdgeInsets.fromLTRB(12, 8, 8, 8),
              child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                Row(children: [
                  Text(folder.split('/').last, style: mono(size: 12.5, color: t.text, weight: FontWeight.w500)),
                  const Spacer(),
                  Flexible(child: Text(folder, overflow: TextOverflow.ellipsis, style: mono(size: 10.5, color: t.faint))),
                ]),
                for (final h in switches.where((h) => h.allowed && !h.entry.contains(':')))
                  Row(children: [
                    Transform.scale(
                      scale: 0.7,
                      child: Switch(
                        key: Key('harness-$folder-${h.entry}'),
                        value: h.on,
                        onChanged: (on) => store.dispatch(HarnessToggled(widget.url, folder, h.entry, on)),
                      ),
                    ),
                    Text(harnessName(h.entry), style: const TextStyle(fontSize: 12.5)),
                    const SizedBox(width: 8),
                    Text(h.on ? 'on' : 'turned off', style: mono(size: 11, color: t.faint)),
                  ]),
              ]),
            ),
          ],
        ]),
      ),
    ]);
  }
}
