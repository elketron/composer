import 'package:flutter/material.dart';

import '../theme/composer_theme.dart';
import '../theme/tokens.dart' as t;

/// `label value`, small and monospace.
class InfoBadge extends StatelessWidget {
  const InfoBadge(this.label, this.value, {super.key});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) => Container(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
        decoration: BoxDecoration(border: Border.all(color: t.border), borderRadius: BorderRadius.circular(t.radiusSm)),
        child: Text.rich(TextSpan(children: [
          TextSpan(text: '$label ', style: mono(size: 11, color: t.faint)),
          TextSpan(text: value, style: mono(size: 11, color: t.text)),
        ])),
      );
}
