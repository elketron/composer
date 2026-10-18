import 'package:flutter/material.dart';

import '../../state/server_status.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import 'page_navigation.dart';

/// A server's sites: open the index, or the micro app.
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
                onTap: () => openPage(context, url, 'site://${site.name}'),
                child: Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 7),
                  child: Text.rich(TextSpan(children: [
                    TextSpan(text: site.title, style: const TextStyle(fontSize: 13)),
                    TextSpan(text: '  ${site.name}', style: mono(size: 11, color: t.faint)),
                  ])),
                ),
              ),
              if (site.hasUi)
                InkWell(
                  key: Key('app-${site.name}'),
                  onTap: () => openApp(context, url, site.name),
                  child: Container(
                    padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 7),
                    decoration: const BoxDecoration(border: Border(left: BorderSide(color: t.border))),
                    child: Text('app', style: mono(size: 11, color: t.accent)),
                  ),
                ),
            ]),
          ),
      ]);
}
