import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../format.dart';

/// A project in a list: its name, what is going on (running, waiting for you, its sessions), its
/// path, and when it was last used.
class ProjectRow extends StatelessWidget {
  const ProjectRow({super.key, required this.project, required this.onTap, this.selected = false});

  final ProjectSummary project;
  final VoidCallback onTap;
  final bool selected;

  @override
  Widget build(BuildContext context) {
    final p = project;
    final color = p.waiting > 0 ? t.warn : (p.running > 0 ? t.info : t.faint);
    final what = [
      if (p.waiting > 0) '${p.waiting} waiting for you',
      if (p.running > 0) '${p.running} running',
      if (p.sessions > 0) '${p.sessions} chat${p.sessions == 1 ? '' : 's'}' else 'no chats',
    ].join(' · ');
    return InkWell(
      key: Key('project-${p.name}'),
      onTap: onTap,
      child: Container(
        color: selected ? t.raised : null,
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 9),
        child: Row(children: [
          Container(width: 7, height: 7, decoration: BoxDecoration(color: color, shape: BoxShape.circle)),
          const SizedBox(width: 10),
          Expanded(
            child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Text(p.name, style: mono(size: 13, color: t.text, weight: FontWeight.w500)),
              Text(what, maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 12, color: t.muted)),
              Text(p.path, maxLines: 1, overflow: TextOverflow.ellipsis, style: mono(size: 10.5, color: t.faint)),
            ]),
          ),
          Text(ago(p.lastUsed ?? p.modified), style: mono(size: 11, color: t.faint)),
        ]),
      ),
    );
  }
}
