import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/types.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../add_server_page.dart';
import '../status_dot.dart';
import '../store_scope.dart';

/// The wide layout's leftmost column: one square per server (its state, and how many things
/// wait for you there), then the server's settings and adding a server.
class ServerRail extends StatelessWidget {
  const ServerRail({super.key});

  static const width = 52.0;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.of(context);
    final state = store.state;
    final active = state.active;
    return Container(
      width: width,
      decoration: const BoxDecoration(color: t.surface, border: Border(right: BorderSide(color: t.border))),
      padding: const EdgeInsets.symmetric(vertical: 12),
      child: Column(children: [
        for (final (i, server) in state.servers.indexed)
          Padding(
            padding: const EdgeInsets.only(bottom: 10),
            child: _ServerSquare(
              key: Key('rail-server-$i'),
              name: server.name,
              on: server.url == active,
              link: state.statusOf(server.url).link,
              waiting: state.statusOf(server.url).approvals.length,
              shortcut: i < 9 ? 'ctrl ${i + 1}' : null,
              onTap: () => store.dispatch(ServerSelected(server.url)),
            ),
          ),
        const Spacer(),
        if (active != null)
          IconButton(
            key: const Key('rail-settings'),
            tooltip: 'This server',
            onPressed: () => store.state.statusOf(active).side == SideTab.server
                ? store.dispatch(SideClosed(active))
                : store.dispatch(SideTabSelected(active, SideTab.server)),
            icon: Icon(Icons.settings_outlined, size: 18, color: state.statusOf(active).side == SideTab.server ? t.text : t.muted),
          ),
        IconButton(
          key: const Key('rail-add-server'),
          tooltip: 'Add a server',
          onPressed: () => AddServerPage.open(context),
          icon: const Icon(Icons.add, size: 18, color: t.muted),
        ),
      ]),
    );
  }
}

class _ServerSquare extends StatelessWidget {
  const _ServerSquare({
    super.key,
    required this.name,
    required this.on,
    required this.link,
    required this.waiting,
    required this.onTap,
    this.shortcut,
  });

  final String name;
  final bool on;
  final LinkState link;
  final int waiting;
  final VoidCallback onTap;
  final String? shortcut;

  @override
  Widget build(BuildContext context) => Tooltip(
        message: shortcut == null ? name : '$name · $shortcut',
        child: InkWell(
          onTap: onTap,
          borderRadius: BorderRadius.circular(t.radius),
          child: SizedBox(
            width: 36,
            height: 36,
            child: Stack(clipBehavior: Clip.none, children: [
              Container(
                alignment: Alignment.center,
                decoration: BoxDecoration(
                  color: on ? t.accent.withValues(alpha: 0.12) : null,
                  border: Border.all(color: on ? t.accent : t.borderStrong),
                  borderRadius: BorderRadius.circular(t.radius),
                ),
                child: Text(
                  name.isEmpty ? '?' : name.substring(0, 1).toUpperCase(),
                  style: mono(size: 13, color: on ? t.accent : t.muted, weight: FontWeight.w600),
                ),
              ),
              Positioned(right: -2, bottom: -2, child: StatusDot(link)),
              if (waiting > 0)
                Positioned(
                  right: -6,
                  top: -6,
                  child: Container(
                    padding: const EdgeInsets.symmetric(horizontal: 3),
                    decoration: BoxDecoration(color: t.warn, borderRadius: BorderRadius.circular(t.radiusSm)),
                    child: Text('$waiting', style: mono(size: 9, color: t.bg, weight: FontWeight.w600)),
                  ),
                ),
            ]),
          ),
        ),
      );
}
