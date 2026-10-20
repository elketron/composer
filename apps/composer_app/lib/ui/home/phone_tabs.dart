import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/types.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';

/// Chats · Pages · Server, with a count on Chats of what needs you.
class PhoneTabs extends StatelessWidget {
  const PhoneTabs({super.key, required this.tab, required this.needsYou});

  final PhoneTab tab;
  final int needsYou;

  @override
  Widget build(BuildContext context) {
    Widget item(PhoneTab value, IconData icon, String label, {int count = 0}) {
      final on = value == tab;
      return Expanded(
        child: InkWell(
          key: Key('tab-${value.name}'),
          onTap: () => StoreScope.read(context).dispatch(TabSelected(value)),
          child: Container(
            decoration: BoxDecoration(border: Border(top: BorderSide(color: on ? t.accent : Colors.transparent, width: 2))),
            child: Column(mainAxisAlignment: MainAxisAlignment.center, children: [
              Badge(
                isLabelVisible: count > 0,
                backgroundColor: t.warn,
                textColor: t.bg,
                label: Text('$count', style: mono(size: 9, color: t.bg, weight: FontWeight.w600)),
                child: Icon(icon, size: 21, color: on ? t.text : t.faint),
              ),
              const SizedBox(height: 3),
              Text(label, style: TextStyle(fontSize: 11, color: on ? t.text : t.faint)),
            ]),
          ),
        ),
      );
    }

    return Container(
      decoration: const BoxDecoration(color: t.surface, border: Border(top: BorderSide(color: t.border))),
      child: SafeArea(
        top: false,
        child: SizedBox(
          height: 58,
          child: Row(children: [
            item(PhoneTab.chats, Icons.chat_bubble_outline, 'Chats', count: needsYou),
            item(PhoneTab.pages, Icons.description_outlined, 'Pages'),
            item(PhoneTab.server, Icons.dns_outlined, 'Server'),
          ]),
        ),
      ),
    );
  }
}
