import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/server_entry.dart';
import '../../state/server_status.dart';
import '../../state/types.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../chat/review_view.dart';
import '../home/server_tab.dart';
import '../pages/reader.dart';
import '../store_scope.dart';
import 'worker_pane.dart';

/// Beside the chat (wide screens): the page reader, a site's app, a ticket's changes, or the
/// server, as tabs. An open app stays loaded while another tab is up. Its left edge drags.
class SidePane extends StatelessWidget {
  const SidePane({super.key, required this.server, required this.status, required this.width, required this.maxWidth});

  static const minWidth = 320.0;

  final ServerEntry server;
  final ServerStatus status;
  final double width;

  /// What the window leaves for it.
  final double maxWidth;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.read(context);
    final url = server.url;
    final side = status.side ?? SideTab.page;
    final app = status.page.appSite;
    final ticket = status.sideTicket;
    final worker = status.sideChat;
    final tabs = <(SideTab, String, String?)>[
      (SideTab.page, 'Page', null),
      if (app != null) (SideTab.app, 'App', app),
      if (ticket != null) (SideTab.changes, 'Changes', ticket),
      if (worker != null) (SideTab.worker, 'Worker', status.session(worker.session)?.ticket ?? '#${worker.session}'),
      if (side == SideTab.server) (SideTab.server, 'Server', null),
    ];
    // Kept built (offstage) so the app doesn't reload on every tab switch.
    final bodies = <SideTab, Widget>{
      SideTab.page: Reader(key: const ValueKey('side-page'), url: url, status: status, show: SideTab.page),
      if (app != null) SideTab.app: Reader(key: ValueKey('side-app-$app'), url: url, status: status, show: SideTab.app),
      if (ticket != null) SideTab.changes: ReviewView(key: ValueKey('side-changes-$ticket'), url: url, ticket: ticket, header: true),
      if (worker != null) SideTab.worker: WorkerPane(key: ValueKey('side-worker-${worker.session}'), url: url, status: status, chat: worker),
      if (side == SideTab.server) SideTab.server: ServerTab(server: server, status: status, wide: true),
    };
    final order = bodies.keys.toList();
    return SizedBox(
      width: width,
      child: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        _ResizeHandle(width: width, maxWidth: maxWidth),
        Expanded(
          child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            Container(
              height: 40,
              padding: const EdgeInsets.only(left: 6, right: 2),
              decoration: const BoxDecoration(color: t.surface, border: Border(bottom: BorderSide(color: t.border))),
              child: Row(crossAxisAlignment: CrossAxisAlignment.end, children: [
                Expanded(
                  child: SingleChildScrollView(
                    scrollDirection: Axis.horizontal,
                    child: Row(crossAxisAlignment: CrossAxisAlignment.end, children: [
                      for (final (tab, label, badge) in tabs)
                        _Tab(
                          key: Key('side-tab-${tab.name}'),
                          label: label,
                          badge: badge,
                          on: tab == side,
                          onTap: () => store.dispatch(SideTabSelected(url, tab, ticket: tab == SideTab.changes ? ticket : null)),
                        ),
                    ]),
                  ),
                ),
                Align(
                  alignment: Alignment.center,
                  child: IconButton(
                    key: const Key('side-close'),
                    tooltip: 'Close (esc)',
                    visualDensity: VisualDensity.compact,
                    onPressed: () => store.dispatch(SideClosed(url)),
                    icon: const Icon(Icons.close, size: 17, color: t.muted),
                  ),
                ),
              ]),
            ),
            Expanded(
              child: IndexedStack(
                index: order.contains(side) ? order.indexOf(side) : 0,
                sizing: StackFit.expand,
                children: [for (final tab in order) bodies[tab]!],
              ),
            ),
          ]),
        ),
      ]),
    );
  }
}

class _Tab extends StatelessWidget {
  const _Tab({super.key, required this.label, required this.on, required this.onTap, this.badge});

  final String label;
  final String? badge;
  final bool on;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => InkWell(
        onTap: onTap,
        child: Container(
          height: 32,
          padding: const EdgeInsets.symmetric(horizontal: 11),
          decoration: BoxDecoration(
            color: on ? t.bg : null,
            border: on ? const Border(top: BorderSide(color: t.border), left: BorderSide(color: t.border), right: BorderSide(color: t.border)) : null,
            borderRadius: const BorderRadius.vertical(top: Radius.circular(t.radius)),
          ),
          child: Row(children: [
            Text(label, style: TextStyle(fontSize: 12.5, color: on ? t.text : t.muted)),
            if (badge != null) ...[
              const SizedBox(width: 6),
              Container(
                padding: const EdgeInsets.symmetric(horizontal: 4),
                decoration: BoxDecoration(border: Border.all(color: t.borderStrong), borderRadius: BorderRadius.circular(t.radiusSm)),
                child: Text(badge!, style: mono(size: 10, color: t.muted)),
              ),
            ],
          ]),
        ),
      );
}

/// The pane's left edge: drag to resize.
class _ResizeHandle extends StatelessWidget {
  const _ResizeHandle({required this.width, required this.maxWidth});

  final double width;
  final double maxWidth;

  @override
  Widget build(BuildContext context) => MouseRegion(
        cursor: SystemMouseCursors.resizeColumn,
        child: GestureDetector(
          behavior: HitTestBehavior.translucent,
          onHorizontalDragUpdate: (d) => StoreScope.read(context).dispatch(
            SideResized((width - d.delta.dx).clamp(SidePane.minWidth, maxWidth)),
          ),
          child: Container(width: 5, decoration: const BoxDecoration(border: Border(left: BorderSide(color: t.border)))),
        ),
      );
}
