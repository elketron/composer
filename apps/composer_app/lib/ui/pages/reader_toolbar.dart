import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/page_state.dart';
import '../../state/server_status.dart';
import '../../state/site_url.dart';
import '../../state/types.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';

/// Back, forward, the URL, follow-agent, page / app, close.
class ReaderToolbar extends StatelessWidget {
  const ReaderToolbar({super.key, required this.url, required this.status, this.onClose});

  final String url;
  final ServerStatus status;
  final VoidCallback? onClose;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.read(context);
    final PageState p = status.page;
    final app = p.appSite != null && status.side == SideTab.app;
    final shown = app ? 'site://${p.appSite}  ·  app' : (p.page?.url ?? p.loading ?? '');
    final site = app ? p.appSite : (p.page == null ? null : splitSiteUrl(p.page!.url).$1);
    Widget button(IconData icon, String tip, VoidCallback? onPressed, {Color? color, Key? key}) => IconButton(
          key: key,
          tooltip: tip,
          visualDensity: VisualDensity.compact,
          onPressed: onPressed,
          icon: Icon(icon, size: 18, color: onPressed == null ? t.faint : (color ?? t.muted)),
        );
    return Container(
      height: 48,
      padding: const EdgeInsets.symmetric(horizontal: 4),
      decoration: const BoxDecoration(color: t.surface, border: Border(bottom: BorderSide(color: t.border))),
      child: Row(children: [
        button(Icons.arrow_back, 'Back', p.back.isEmpty ? null : () => store.dispatch(PageBack(url))),
        button(Icons.arrow_forward, 'Forward', p.forward.isEmpty ? null : () => store.dispatch(PageForward(url))),
        const SizedBox(width: 6),
        Expanded(child: Text(shown, overflow: TextOverflow.ellipsis, style: mono(size: 12, color: t.muted))),
        if (site != null && status.hasApp(site))
          !app
              ? TextButton(key: const Key('open-app'), onPressed: () => store.dispatch(AppRequested(url, site)), child: const Text('App'))
              : TextButton(onPressed: () => store.dispatch(PageRequested(url, 'site://$site')), child: const Text('Page')),
        Tooltip(
          message: p.follow ? 'Following the agent: pages it opens show here' : 'Not following the agent',
          child: TextButton.icon(
            key: const Key('follow'),
            onPressed: () => store.dispatch(FollowToggled(url)),
            style: TextButton.styleFrom(foregroundColor: p.follow ? t.accent : t.faint),
            icon: Icon(p.follow ? Icons.link : Icons.link_off, size: 16),
            label: Text('follow', style: mono(size: 11, color: p.follow ? t.accent : t.faint)),
          ),
        ),
        if (onClose != null) button(Icons.close, 'Close', onClose),
      ]),
    );
  }
}
