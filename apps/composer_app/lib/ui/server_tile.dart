import 'package:flutter/material.dart';

import '../state/server_entry.dart';
import '../state/server_status.dart';
import '../state/types.dart';
import '../theme/composer_theme.dart';
import '../theme/tokens.dart' as t;
import 'format.dart';
import 'status_dot.dart';

/// A server in the switcher: name, link state, and the model (or why it is offline).
class ServerTile extends StatelessWidget {
  const ServerTile({super.key, required this.server, required this.status, required this.selected, required this.onTap, this.shortcut});

  final ServerEntry server;
  final ServerStatus status;
  final bool selected;
  final VoidCallback onTap;

  /// `ctrl 1` etc., shown on desktop.
  final String? shortcut;

  @override
  Widget build(BuildContext context) {
    final detail = switch (status.link) {
      LinkState.offline => status.error ?? 'offline',
      _ => status.info == null ? shortUrl(server.url) : '${status.info!.model} · ${status.info!.slots} slots',
    };
    return Material(
      color: selected ? t.raised : Colors.transparent,
      borderRadius: BorderRadius.circular(t.radius),
      child: InkWell(
        borderRadius: BorderRadius.circular(t.radius),
        onTap: onTap,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
          child: Row(children: [
            StatusDot(status.link),
            const SizedBox(width: 10),
            Expanded(
              child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                Text(server.name, maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 14, fontWeight: FontWeight.w500)),
                const SizedBox(height: 2),
                Text(
                  detail,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: mono(size: 11, color: status.link == LinkState.offline ? t.err : t.faint),
                ),
              ]),
            ),
            if (shortcut != null) Text(shortcut!, style: mono(size: 10, color: t.faint)),
          ]),
        ),
      ),
    );
  }
}
