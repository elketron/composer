import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/desk_view.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';
import 'desk_tab_page.dart';

/// The session's desk: apps, pages and changes the agent (or you) put beside the chat. Terminals
/// stay on the desktop. A tap shows the tab full screen, its chat bar still at the bottom.
void showDesk(BuildContext context, String url) => showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      showDragHandle: true,
      builder: (_) => DeskSheet(url: url, parent: context),
    );

class DeskSheet extends StatelessWidget {
  const DeskSheet({super.key, required this.url, required this.parent});

  final String url;
  final BuildContext parent;

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.of(context);
    final chat = store.state.statusOf(url).chat;
    final desk = DeskView(chat?.desk ?? const Desk());
    return SafeArea(
      top: false,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(14, 0, 14, 14),
        child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Row(children: [
            Text('Desk', style: mono(size: 13, color: t.faint, weight: FontWeight.w600)),
            const Spacer(),
            if (chat != null) Text(chat.session, style: mono(size: 11, color: t.faint)),
          ]),
          const SizedBox(height: 8),
          if (desk.tabs.isEmpty)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 20),
              child: Text(
                'Nothing here yet. What the agent opens with show_page or show_app lands here.',
                textAlign: TextAlign.center,
                style: const TextStyle(fontSize: 13, color: t.faint),
              ),
            )
          else
            Flexible(
              child: SingleChildScrollView(
                child: Column(children: [
                  for (final entry in desk.tabs)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 4),
                      child: Material(
                        color: entry.id == desk.shown?.id ? t.raised : t.surface,
                        borderRadius: BorderRadius.circular(t.radius),
                        child: InkWell(
                          key: Key('desk-tab-${entry.id}'),
                          borderRadius: BorderRadius.circular(t.radius),
                          onTap: () {
                            store.dispatch(DeskTabFocused(url, entry.id));
                            Navigator.pop(context);
                            DeskTabScreen.open(parent, url, entry);
                          },
                          child: Padding(
                            padding: const EdgeInsets.fromLTRB(10, 8, 2, 8),
                            child: Row(children: [
                              Icon(DeskView.icon(entry), size: 17, color: entry.unseen ? t.accent : t.muted),
                              const SizedBox(width: 9),
                              Expanded(
                                child: Text(
                                  DeskView.label(entry),
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: TextStyle(fontSize: 13.5, color: t.text, fontWeight: entry.unseen ? FontWeight.w600 : FontWeight.w400),
                                ),
                              ),
                              if (entry.unseen) Text('new', style: mono(size: 10, color: t.accent)),
                              IconButton(
                                key: Key('desk-close-${entry.id}'),
                                tooltip: 'Close this tab',
                                visualDensity: VisualDensity.compact,
                                icon: const Icon(Icons.close, size: 16, color: t.faint),
                                onPressed: () => store.dispatch(DeskTabClosed(url, entry.id)),
                              ),
                            ]),
                          ),
                        ),
                      ),
                    ),
                ]),
              ),
            ),
          const SizedBox(height: 6),
          Text(
            'Open a page from Pages: the reader can put it on the desk.',
            style: const TextStyle(fontSize: 11.5, color: t.faint),
          ),
        ]),
      ),
    );
  }
}
