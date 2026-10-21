import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;

/// Agent, id, state and context use of the open session.
class ChatHeader extends StatelessWidget {
  const ChatHeader({super.key, required this.sessionId, this.session, this.parent, this.onClose, this.onBrowse, this.onClearProject, this.menu});

  final String sessionId;
  final SessionInfo? session;

  /// The orchestrator, for a worker.
  final SessionInfo? parent;
  final VoidCallback? onClose;

  /// Open the sites (site://home) in the reader.
  final VoidCallback? onBrowse;

  /// Unset the session's project.
  final VoidCallback? onClearProject;

  /// ⋮ (wide layout).
  final Widget? menu;

  @override
  Widget build(BuildContext context) {
    final s = session;
    return Container(
      height: 48,
      padding: const EdgeInsets.only(left: 16, right: 6),
      decoration: const BoxDecoration(color: t.surface, border: Border(bottom: BorderSide(color: t.border))),
      child: Row(children: [
        Expanded(
          child: Text.rich(
            TextSpan(children: [
              if (parent != null) TextSpan(text: '${parent!.agent} › ', style: mono(size: 14, color: t.faint)),
              TextSpan(text: s?.agent ?? 'session', style: mono(size: 14, color: t.text, weight: FontWeight.w600)),
              TextSpan(text: '  #$sessionId', style: mono(size: 12, color: t.faint)),
              if (s?.ticket != null) TextSpan(text: '  ticket ${s!.ticket}', style: mono(size: 12, color: t.info)),
              if (s != null && s.state != TurnState.idle)
                TextSpan(text: '  ${s.state.wire.replaceAll('_', ' ')}', style: mono(size: 11, color: t.warn)),
            ]),
            overflow: TextOverflow.ellipsis,
          ),
        ),
        if (s != null && s.contextTokens > 0)
          Tooltip(
            message: 'Context in use (prompt tokens of the last model call)',
            child: Text('${(s.contextTokens / 1000).toStringAsFixed(1)}k ctx', style: mono(size: 11, color: t.faint)),
          ),
        if (s?.project != null) ...[
          const SizedBox(width: 10),
          Tooltip(message: s!.project!, child: Text(s.project!.split('/').last, style: mono(size: 11, color: t.faint))),
          if (onClearProject != null)
            IconButton(
              key: const Key('clear-project'),
              tooltip: 'Unset the project',
              visualDensity: VisualDensity.compact,
              onPressed: onClearProject,
              icon: const Icon(Icons.close, size: 13, color: t.faint),
            ),
        ],
        if (onBrowse != null)
          IconButton(
            key: const Key('browse'),
            tooltip: 'Sites',
            onPressed: onBrowse,
            icon: const Icon(Icons.menu_book_outlined, size: 18, color: t.muted),
          ),
        ?menu,
        if (onClose != null)
          IconButton(tooltip: 'Close the chat', onPressed: onClose, icon: const Icon(Icons.close, size: 18, color: t.muted)),
      ]),
    );
  }
}
