import 'package:flutter/material.dart';

import '../../state/server_status.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import 'page_navigation.dart';

/// A server's sites. A site with a built app opens the app; its pages are one tap to the side.
class SiteList extends StatelessWidget {
  const SiteList({super.key, required this.url, required this.status});

  final String url;
  final ServerStatus status;

  @override
  Widget build(BuildContext context) => Wrap(spacing: 8, runSpacing: 8, children: [
        for (final site in status.sites)
          Container(
            decoration: BoxDecoration(border: Border.all(color: t.border), borderRadius: BorderRadius.circular(t.radius)),
            child: Row(mainAxisSize: MainAxisSize.min, children: [
              InkWell(
                key: Key('site-${site.name}'),
                borderRadius: BorderRadius.circular(t.radius),
                onTap: () => site.appBuilt ? openApp(context, url, site.name) : openPage(context, url, 'site://${site.name}'),
                child: Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 7),
                  child: Text.rich(TextSpan(children: [
                    TextSpan(text: site.title, style: const TextStyle(fontSize: 13)),
                    TextSpan(text: '  ${site.name}', style: mono(size: 11, color: t.faint)),
                    if (site.appBuilt) TextSpan(text: '  app', style: mono(size: 11, color: t.accent)),
                  ])),
                ),
              ),
              if (site.hasUi && !site.appBuilt)
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 7),
                  child: Text('app not built', style: mono(size: 11, color: t.faint)),
                ),
              if (site.appBuilt)
                InkWell(
                  key: Key('pages-${site.name}'),
                  onTap: () => openPage(context, url, 'site://${site.name}'),
                  child: Container(
                    padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 7),
                    decoration: const BoxDecoration(border: Border(left: BorderSide(color: t.border))),
                    child: Text('pages', style: mono(size: 11, color: t.muted)),
                  ),
                ),
            ]),
          ),
      ]);
}
