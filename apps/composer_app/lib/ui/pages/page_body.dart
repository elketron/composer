import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';
import 'package:flutter_markdown_plus/flutter_markdown_plus.dart';
import 'package:url_launcher/url_launcher.dart';

import '../../state/site_url.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../chat/agent_text.dart';
import 'page_navigation.dart';

/// A page: title, frontmatter, the markdown (links open in the reader), backlinks.
class PageBody extends StatelessWidget {
  const PageBody({super.key, required this.url, required this.token, required this.page});

  final String url;
  final String token;
  final PageResponse page;

  /// The markdown without its frontmatter block.
  static String body(String raw) {
    final match = RegExp(r'^---\r?\n[\s\S]*?\r?\n---[ \t]*(\r?\n|$)').firstMatch(raw);
    return match == null ? raw : raw.substring(match.end);
  }

  /// Most pages open with `# Title`; the title is shown separately only for those that don't.
  static bool startsWithHeading(String markdown) =>
      markdown.trimLeft().startsWith(RegExp(r'#[ \t]'));

  void _tap(BuildContext context, String? href) {
    if (href == null) return;
    switch (resolveLink(href, page.url)) {
      case SiteLink(url: final target):
        openPage(context, url, target);
      case ExternalLink(url: final target):
        launchUrl(Uri.parse(target), mode: LaunchMode.externalApplication);
      case InvalidLink():
        break;
    }
  }

  Widget _image(Uri uri, String? title, String? alt) {
    final target = resolveLink(uri.toString(), page.url);
    if (target is ExternalLink) return Image.network(target.url);
    if (target is! SiteLink) return Text(alt ?? '');
    final (site, path) = splitSiteUrl(withoutFragment(target.url));
    return Image.network(
      '$url/api/files/$site/$path',
      headers: {'Authorization': 'Bearer $token'},
      errorBuilder: (_, _, _) => Text('[image: ${alt ?? path}]', style: mono(size: 12, color: t.faint)),
    );
  }

  @override
  Widget build(BuildContext context) {
    final meta = page.frontmatter is Map
        ? (page.frontmatter as Map).entries.where((e) => e.key != 'title' && e.value != null).toList()
        : const <MapEntry<dynamic, dynamic>>[];
    final markdown = body(page.raw);
    return SelectionArea(
      child: ListView(padding: const EdgeInsets.fromLTRB(20, 18, 20, 28), children: [
        if (!startsWithHeading(markdown)) Text(page.title, style: const TextStyle(fontSize: 20, fontWeight: FontWeight.w600)),
        if (meta.isNotEmpty) ...[
          const SizedBox(height: 8),
          Wrap(spacing: 14, runSpacing: 4, children: [
            for (final entry in meta)
              Text.rich(TextSpan(children: [
                TextSpan(text: '${entry.key} ', style: mono(size: 11, color: t.faint)),
                TextSpan(text: entry.value is List ? (entry.value as List).join(', ') : '${entry.value}', style: mono(size: 11, color: t.muted)),
              ])),
          ]),
        ],
        if (page.frontmatterError != null) ...[
          const SizedBox(height: 6),
          Text('frontmatter: ${page.frontmatterError}', style: mono(size: 11, color: t.err)),
        ],
        const SizedBox(height: 14),
        MarkdownBody(
          data: markdown,
          styleSheet: AgentText.styleSheet(context),
          onTapLink: (_, href, _) => _tap(context, href),
          imageBuilder: _image,
        ),
        if (page.backlinks.isNotEmpty) ...[
          const SizedBox(height: 28),
          const Divider(),
          const SizedBox(height: 10),
          Text('LINKED FROM', style: mono(size: 11, color: t.faint, weight: FontWeight.w600)),
          const SizedBox(height: 6),
          for (final link in page.backlinks)
            Align(
              alignment: Alignment.centerLeft,
              child: TextButton(
                style: TextButton.styleFrom(padding: const EdgeInsets.symmetric(horizontal: 4), minimumSize: const Size(0, 30)),
                onPressed: () => openPage(context, url, link),
                child: Text(link, style: mono(size: 12, color: t.accent)),
              ),
            ),
        ],
      ]),
    );
  }
}
