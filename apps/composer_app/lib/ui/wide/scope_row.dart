import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/projects.dart';
import '../../state/server_status.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../projects/project_picker.dart';
import '../store_scope.dart';

/// Above the session list: All, the few projects with something going on, the current scope, and
/// … to search all projects. Picking one narrows the list and the palette to it.
class ScopeRow extends StatelessWidget {
  const ScopeRow({super.key, required this.url, required this.status});

  /// Active projects shown at most.
  static const shown = 3;

  final String url;
  final ServerStatus status;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.read(context);
    final scope = status.scope;
    final active = [
      for (final p in status.projects)
        if ((p.running > 0 || p.waiting > 0) && p.folder != null) p.folder!,
    ].take(shown).toList();
    final folders = [...active, if (scope != null && !active.contains(scope)) scope];

    Widget chip(String label, bool on, VoidCallback onTap, {Key? key}) => InkWell(
          key: key,
          onTap: onTap,
          borderRadius: BorderRadius.circular(t.radius),
          child: Container(
            padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
            decoration: BoxDecoration(
              color: on ? t.accent.withValues(alpha: 0.12) : null,
              border: Border.all(color: on ? t.accent : t.border),
              borderRadius: BorderRadius.circular(t.radius),
            ),
            child: Text(label, style: mono(size: 11.5, color: on ? t.text : t.muted)),
          ),
        );
    return Padding(
      padding: const EdgeInsets.fromLTRB(10, 8, 10, 0),
      child: Wrap(spacing: 4, runSpacing: 4, children: [
        chip('All', scope == null, () => store.dispatch(ScopeChanged(url, null)), key: const Key('scope-all')),
        for (final f in folders)
          chip(projectName(f), f == scope, () => store.dispatch(ScopeChanged(url, f == scope ? null : f)), key: Key('scope-${projectName(f)}')),
        chip('…', false, () {
          showProjectPicker(context, url, (p) => store.dispatch(ScopeChanged(url, p.folder ?? p.path)), selected: scope);
        }, key: const Key('scope-more')),
      ]),
    );
  }
}
