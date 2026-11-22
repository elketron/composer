import 'package:flutter/material.dart';

import '../../state/apps.dart';
import '../../state/server_status.dart';
import '../../theme/tokens.dart' as t;
import 'app_screen.dart';

/// The app tabs on the open session's desk (the agent's `show_page app://…`, or opened by hand),
/// as a row of chips under the site row, each labelled with the app's title; one the agent opened
/// that you have not looked at carries a dot. Tap: the app's screen. Nothing when there are none.
class AppTabsRow extends StatelessWidget {
  const AppTabsRow({super.key, required this.url, required this.status, this.viewBuilder});

  final String url;
  final ServerStatus status;
  final AppViewBuilder? viewBuilder;

  @override
  Widget build(BuildContext context) {
    final desk = status.chat?.desk;
    final tabs = desk == null ? const <Never>[] : appTabs(desk);
    if (tabs.isEmpty) return const SizedBox.shrink();
    return Container(
      key: const Key('app-tabs'),
      height: 44,
      decoration: const BoxDecoration(color: t.bg, border: Border(bottom: BorderSide(color: t.border))),
      child: ListView(
        scrollDirection: Axis.horizontal,
        padding: const EdgeInsets.symmetric(horizontal: 14),
        children: [
          for (final (entry, app) in tabs)
            Padding(
              padding: const EdgeInsets.only(right: 8),
              child: _AppChip(
                key: Key('app-tab-${app.name}'),
                title: appTitle(status.info, app.name),
                unseen: entry.unseen,
                onTap: () => openApp(context, url, app.name, viewBuilder: viewBuilder),
              ),
            ),
        ],
      ),
    );
  }
}

class _AppChip extends StatelessWidget {
  const _AppChip({super.key, required this.title, required this.unseen, required this.onTap});

  final String title;
  final bool unseen;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => Semantics(
        button: true,
        label: 'App $title${unseen ? ', new' : ''}: open',
        excludeSemantics: true,
        child: InkWell(
          borderRadius: BorderRadius.circular(t.radiusControl),
          onTap: onTap,
          child: Center(
            widthFactor: 1,
            child: Container(
              height: 30,
              padding: const EdgeInsets.symmetric(horizontal: 10),
              decoration: BoxDecoration(
                border: Border.all(color: t.borderStrong),
                borderRadius: BorderRadius.circular(t.radiusControl),
              ),
              child: Row(mainAxisSize: MainAxisSize.min, children: [
                const Icon(Icons.web_asset, size: 14, color: t.muted),
                const SizedBox(width: 6),
                ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 160),
                  child: Text(title, maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 12, color: t.text)),
                ),
                if (unseen) ...[
                  const SizedBox(width: 6),
                  Container(
                    key: const Key('app-tab-unseen'),
                    width: 6,
                    height: 6,
                    decoration: BoxDecoration(color: t.accent, borderRadius: BorderRadius.circular(t.radiusDot)),
                  ),
                ],
              ]),
            ),
          ),
        ),
      );
}
