import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/server_status.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../pages/page_navigation.dart';
import '../store_scope.dart';
import 'section_label.dart';

/// The workspace: search (the agents' query syntax), what agents and apps changed lately,
/// and the sites. A site with a built app opens the app; its pages are one tap to the side.
class PagesTab extends StatefulWidget {
  const PagesTab({super.key, required this.url, required this.status});

  final String url;
  final ServerStatus status;

  @override
  State<PagesTab> createState() => _PagesTabState();
}

class _PagesTabState extends State<PagesTab> {
  late final _query = TextEditingController(text: widget.status.search.query);

  @override
  void dispose() {
    _query.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final status = widget.status;
    final search = status.search;
    final searching = search.query.trim().isNotEmpty;
    return ListView(
      key: const PageStorageKey('pages'),
      padding: const EdgeInsets.fromLTRB(14, 12, 14, 24),
      children: [
        TextField(
          key: const Key('search'),
          controller: _query,
          textInputAction: TextInputAction.search,
          style: const TextStyle(fontSize: 14),
          decoration: InputDecoration(
            hintText: 'Search pages, or type:ticket status:open',
            prefixIcon: const Icon(Icons.search, size: 18, color: t.faint),
            suffixIcon: searching
                ? IconButton(
                    icon: const Icon(Icons.close, size: 16, color: t.faint),
                    onPressed: () {
                      _query.clear();
                      StoreScope.read(context).dispatch(SearchChanged(widget.url, ''));
                    },
                  )
                : null,
            contentPadding: const EdgeInsets.symmetric(vertical: 11),
          ),
          onChanged: (q) => StoreScope.read(context).dispatch(SearchChanged(widget.url, q)),
        ),
        if (searching) ...[
          SectionLabel(search.hits == null ? 'Searching…' : '${search.hits!.length} found'),
          if (search.error != null) Text(search.error!, style: const TextStyle(color: t.err, fontSize: 13)),
          if (search.hits != null)
            _Group([
              for (final hit in search.hits!)
                _Row(
                  title: hit.title,
                  detail: hit.url.replaceFirst('site://', ''),
                  onTap: () => openPage(context, widget.url, hit.url),
                ),
            ]),
        ] else ...[
          if (status.recent.isNotEmpty) ...[
            const SectionLabel('Recently changed by agents'),
            _Group([
              for (final page in status.recent.take(6))
                _Row(
                  title: _title(page.url),
                  detail: '${page.url.replaceFirst('site://', '')} · ${page.by == ChangedBy.app ? 'app' : 'agent'}',
                  onTap: () => openPage(context, widget.url, page.url),
                ),
            ]),
          ],
          const SectionLabel('Sites'),
          _Group([
            for (final site in status.sites)
              _Row(
                key: Key('site-${site.name}'),
                title: site.title,
                badge: site.appBuilt ? 'app' : null,
                detail: (site.description ?? '').isNotEmpty ? site.description! : 'site://${site.name}',
                onTap: () => site.appBuilt ? openApp(context, widget.url, site.name) : openPage(context, widget.url, 'site://${site.name}'),
                trailing: site.appBuilt
                    ? TextButton(
                        key: Key('pages-${site.name}'),
                        style: TextButton.styleFrom(foregroundColor: t.muted, visualDensity: VisualDensity.compact),
                        onPressed: () => openPage(context, widget.url, 'site://${site.name}'),
                        child: const Text('pages'),
                      )
                    : Text(
                        site.hasUi ? 'app not built' : '${site.pages}',
                        style: mono(size: 11, color: t.faint),
                      ),
              ),
          ]),
        ],
      ],
    );
  }

  /// `site://memory/topics/release.md` → `release`.
  static String _title(String url) {
    final last = url.split('/').where((s) => s.isNotEmpty).last;
    final name = last == 'index.md' ? url.split('/').reversed.skip(1).first : last;
    return name.replaceFirst(RegExp(r'\.md$'), '');
  }
}

class _Group extends StatelessWidget {
  const _Group(this.rows);
  final List<Widget> rows;

  @override
  Widget build(BuildContext context) => Container(
        decoration: BoxDecoration(color: t.surface, border: Border.all(color: t.border), borderRadius: BorderRadius.circular(t.radius)),
        child: Column(children: [
          for (final (i, row) in rows.indexed) ...[if (i > 0) const Divider(height: 1), row],
        ]),
      );
}

class _Row extends StatelessWidget {
  const _Row({super.key, required this.title, required this.detail, required this.onTap, this.trailing, this.badge});

  final String title;
  final String detail;
  final VoidCallback onTap;
  final Widget? trailing;
  final String? badge;

  @override
  Widget build(BuildContext context) => InkWell(
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.fromLTRB(12, 10, 8, 10),
          child: Row(children: [
            Expanded(
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                Row(children: [
                  Flexible(child: Text(title, maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 14))),
                  if (badge != null) ...[
                    const SizedBox(width: 6),
                    Container(
                      padding: const EdgeInsets.symmetric(horizontal: 5),
                      decoration: BoxDecoration(border: Border.all(color: t.accent.withValues(alpha: 0.4)), borderRadius: BorderRadius.circular(t.radiusSm)),
                      child: Text(badge!, style: mono(size: 10.5, color: t.accent)),
                    ),
                  ],
                ]),
                const SizedBox(height: 1),
                Text(detail, maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 12, color: t.muted)),
              ]),
            ),
            if (trailing != null) ...[const SizedBox(width: 8), trailing!],
          ]),
        ),
      );
}
