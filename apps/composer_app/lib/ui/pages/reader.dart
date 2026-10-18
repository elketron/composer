import 'package:flutter/material.dart';

import '../../state/server_status.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';
import 'app_view.dart';
import 'page_body.dart';
import 'reader_toolbar.dart';

/// The page reader of a server: toolbar, then the page or the site's micro app.
class Reader extends StatelessWidget {
  const Reader({super.key, required this.url, required this.status, this.onClose});

  final String url;
  final ServerStatus status;
  final VoidCallback? onClose;

  @override
  Widget build(BuildContext context) {
    final state = StoreScope.of(context).state;
    final token = state.servers.where((s) => s.url == url).firstOrNull?.token ?? '';
    final p = status.page;
    final Widget body;
    if (p.appSite != null) {
      body = AppView(key: ValueKey('app-$url-${p.appSite}'), url: url, token: token, site: p.appSite!, session: status.chat?.session);
    } else if (p.page != null) {
      body = PageBody(key: ValueKey(p.page!.url), url: url, token: token, page: p.page!);
    } else if (p.loading != null) {
      body = const Center(child: SizedBox(width: 18, height: 18, child: CircularProgressIndicator(strokeWidth: 2)));
    } else {
      body = const SizedBox.shrink();
    }
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      ReaderToolbar(url: url, status: status, onClose: onClose),
      if (p.loading != null && p.page != null && p.appSite == null)
        const LinearProgressIndicator(minHeight: 2, color: t.accent, backgroundColor: Colors.transparent),
      if (p.error != null)
        Container(
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
          color: t.err.withValues(alpha: 0.08),
          child: Text(p.error!, key: const Key('page-error'), style: mono(size: 12, color: t.err)),
        ),
      Expanded(child: body),
    ]);
  }
}
