import 'package:flutter/material.dart';
import 'package:flutter_markdown_plus/flutter_markdown_plus.dart';
import 'package:url_launcher/url_launcher.dart';

import '../../state/actions.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';

/// The agent's reply, as markdown. Web links open in the browser.
class AgentText extends StatelessWidget {
  const AgentText(this.text, {super.key, this.streaming = false});

  final String text;

  /// Still arriving: shows a caret.
  final bool streaming;

  static MarkdownStyleSheet styleSheet(BuildContext context) {
    const body = TextStyle(fontSize: 15, height: 1.5, color: t.text);
    return MarkdownStyleSheet.fromTheme(Theme.of(context)).copyWith(
      p: body,
      listBullet: body,
      a: const TextStyle(color: t.accent),
      strong: const TextStyle(fontWeight: FontWeight.w600),
      code: mono(size: 13, color: t.muted).copyWith(backgroundColor: Colors.transparent),
      codeblockDecoration: BoxDecoration(
        color: t.surface,
        border: Border.all(color: t.border),
        borderRadius: BorderRadius.circular(t.radius),
      ),
      codeblockPadding: const EdgeInsets.all(10),
      blockquoteDecoration: const BoxDecoration(border: Border(left: BorderSide(color: t.borderStrong, width: 2))),
      blockquotePadding: const EdgeInsets.only(left: 10),
      h1: const TextStyle(fontSize: 19, fontWeight: FontWeight.w600, height: 1.4),
      h2: const TextStyle(fontSize: 17, fontWeight: FontWeight.w600, height: 1.4),
      h3: const TextStyle(fontSize: 15, fontWeight: FontWeight.w600, height: 1.4),
      tableBorder: TableBorder.all(color: t.border),
      tableHead: const TextStyle(fontWeight: FontWeight.w600),
      horizontalRuleDecoration: const BoxDecoration(border: Border(top: BorderSide(color: t.border))),
    );
  }

  @override
  Widget build(BuildContext context) => MarkdownBody(
        data: streaming ? '$text ▍' : text,
        selectable: true,
        styleSheet: styleSheet(context),
        onTapLink: (_, href, _) => _open(context, href),
      );

  void _open(BuildContext context, String? href) {
    if (href == null) return;
    final uri = Uri.tryParse(href);
    if (uri != null && (uri.scheme == 'http' || uri.scheme == 'https')) {
      launchUrl(uri, mode: LaunchMode.externalApplication);
    } else {
      StoreScope.read(context).dispatch(NoticeShown('$href: pages open in the app with the page reader (next milestone)'));
    }
  }
}
