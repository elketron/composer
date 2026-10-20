import 'package:flutter/material.dart';

import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;

/// `NEEDS YOU`, `RUNNING`: a list section's heading, with something on the right.
class SectionLabel extends StatelessWidget {
  const SectionLabel(this.text, {super.key, this.trailing, this.top = 18});

  final String text;
  final Widget? trailing;
  final double top;

  @override
  Widget build(BuildContext context) => Padding(
        padding: EdgeInsets.fromLTRB(2, top, 2, 8),
        child: Row(children: [
          Expanded(child: Text(text.toUpperCase(), style: mono(size: 10.5, color: t.faint).copyWith(letterSpacing: 0.8))),
          ?trailing,
        ]),
      );
}
