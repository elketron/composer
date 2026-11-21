import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';

/// Opens the site picker for the open chat.
void showSiteSheet(BuildContext context, String url) {
  // The server's sites may have changed since the catalog was fetched.
  StoreScope.read(context).dispatch(CatalogRequested(url));
  showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    showDragHandle: true,
    builder: (_) => SiteSheet(url: url),
  );
}

/// The server's sites, the current one marked, and a way to leave it. Choosing one sends
/// `site.set` for the open chat; the reply's sentence comes back as a notice.
class SiteSheet extends StatelessWidget {
  const SiteSheet({super.key, required this.url});

  final String url;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.of(context);
    final status = store.state.statusOf(url);
    final chat = status.chat;
    final current = chat == null ? null : status.session(chat.session)?.site;
    final sites = status.sites;

    void choose(String? site) {
      Navigator.pop(context);
      if (site != current) store.dispatch(SiteChosen(url, site));
    }

    Widget row(String name, {required String title, String? detail, required bool on, Key? key, Color? color}) => InkWell(
          key: key,
          onTap: () => choose(name.isEmpty ? null : name),
          child: Container(
            constraints: const BoxConstraints(minHeight: 52),
            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8),
            decoration: const BoxDecoration(border: Border(top: BorderSide(color: t.border))),
            child: Row(children: [
              SizedBox(width: 22, child: on ? const Icon(Icons.check, size: 18, color: t.accent) : null),
              Expanded(
                child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                  Text.rich(TextSpan(children: [
                    TextSpan(text: name.isEmpty ? title : name, style: mono(size: 13.5, color: color ?? (on ? t.accent : t.text), weight: FontWeight.w600)),
                    if (name.isNotEmpty && title.isNotEmpty && title != name) TextSpan(text: '  $title', style: const TextStyle(fontSize: 12.5, color: t.muted)),
                  ])),
                  if (detail != null) Text(detail, style: mono(size: 11, color: t.faint)),
                ]),
              ),
            ]),
          ),
        );

    return SafeArea(
      child: ConstrainedBox(
        constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(context).height * 0.7),
        child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(14, 0, 14, 10),
            child: Text('Site', style: mono(size: 13, color: t.faint, weight: FontWeight.w600)),
          ),
          Flexible(
            child: ListView(shrinkWrap: true, padding: EdgeInsets.zero, children: [
              if (sites.isEmpty)
                Padding(
                  key: const Key('site-sheet-empty'),
                  padding: const EdgeInsets.fromLTRB(14, 8, 14, 16),
                  child: Text(
                    'No sites in this server.',
                    style: const TextStyle(fontSize: 13, color: t.faint),
                  ),
                ),
              for (final site in sites)
                row(
                  site.name,
                  title: site.title,
                  detail: site.mcp.isEmpty ? '' : 'mcp ${site.mcp.join(', ')}',
                  on: site.name == current,
                  key: Key('site-pick-${site.name}'),
                ),
              if (current != null)
                row('', title: 'Leave ${status.sites.any((w) => w.name == current) ? 'the site' : current}', on: false, key: const Key('site-leave'), color: t.err),
            ]),
          ),
        ]),
      ),
    );
  }
}
