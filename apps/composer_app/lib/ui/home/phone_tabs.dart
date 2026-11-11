import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/types.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';

/// The bottom bar: inbox · workflows · pages · server. Icons over lowercase labels, the active tab
/// in the accent colour under a 2px line; the inbox label carries the count of what needs you.
/// Square, edge to edge.
class PhoneTabs extends StatelessWidget {
  const PhoneTabs({super.key, required this.tab, required this.needsYou});

  final PhoneTab tab;
  final int needsYou;

  @override
  Widget build(BuildContext context) {
    Widget item(PhoneTab value, IconData icon, String label) {
      final on = value == tab;
      final color = on ? t.accent : t.muted;
      return Expanded(
        child: Semantics(
          button: true,
          selected: on,
          label: label,
          excludeSemantics: true,
          child: InkWell(
            key: Key('tab-${value.name}'),
            onTap: () => StoreScope.read(context).dispatch(TabSelected(value)),
            child: Container(
              decoration: BoxDecoration(border: Border(top: BorderSide(color: on ? t.accent : t.surface, width: 2))),
              child: Column(mainAxisAlignment: MainAxisAlignment.center, children: [
                Icon(icon, size: 18, color: color),
                const SizedBox(height: 3),
                Text(label, maxLines: 1, style: TextStyle(fontSize: 11, color: color, fontWeight: on ? FontWeight.w600 : FontWeight.w400)),
              ]),
            ),
          ),
        ),
      );
    }

    return Container(
      decoration: const BoxDecoration(color: t.surface, border: Border(top: BorderSide(color: t.border))),
      child: SafeArea(
        top: false,
        child: SizedBox(
          height: 56,
          child: Row(children: [
            item(PhoneTab.inbox, Icons.inbox_outlined, needsYou > 0 ? 'inbox · $needsYou' : 'inbox'),
            item(PhoneTab.workflows, Icons.grid_view_outlined, 'workflows'),
            item(PhoneTab.pages, Icons.description_outlined, 'pages'),
            item(PhoneTab.server, Icons.dns_outlined, 'server'),
          ]),
        ),
      ),
    );
  }
}
