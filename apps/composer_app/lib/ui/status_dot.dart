import 'package:flutter/widgets.dart';

import '../state/types.dart';
import '../theme/tokens.dart' as t;

/// A server's link state: green online, amber connecting, red offline.
class StatusDot extends StatelessWidget {
  const StatusDot(this.link, {super.key});

  final LinkState link;

  @override
  Widget build(BuildContext context) => Container(
        width: 7,
        height: 7,
        decoration: BoxDecoration(
          color: switch (link) {
            LinkState.online => t.ok,
            LinkState.connecting => t.warn,
            LinkState.offline => t.err,
          },
          borderRadius: BorderRadius.circular(t.radiusSm),
        ),
      );
}
