import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../format.dart';
import '../store_scope.dart';

/// The open session's background processes that are running: each command, since when, and a
/// stop. Nothing when there are none.
class ProcessesBar extends StatelessWidget {
  const ProcessesBar({super.key, required this.url, required this.processes});

  final String url;
  final List<ProcessInfo> processes;

  @override
  Widget build(BuildContext context) {
    if (processes.isEmpty) return const SizedBox.shrink();
    return Container(
      key: const Key('processes'),
      decoration: const BoxDecoration(color: t.surface, border: Border(top: BorderSide(color: t.border))),
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 2),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        for (final p in processes)
          Row(key: Key('process-${p.terminal}'), children: [
            const Icon(Icons.play_arrow, size: 14, color: t.ok),
            const SizedBox(width: 6),
            Expanded(
              child: Tooltip(
                message: '${p.command}\nin ${p.cwd}',
                child: Text(p.command, maxLines: 1, overflow: TextOverflow.ellipsis, style: mono(size: 12, color: t.text)),
              ),
            ),
            const SizedBox(width: 8),
            Text(ago(p.started), style: mono(size: 11, color: t.faint)),
            IconButton(
              key: Key('stop-process-${p.terminal}'),
              tooltip: 'Stop the process',
              visualDensity: VisualDensity.compact,
              icon: const Icon(Icons.stop_circle_outlined, size: 18, color: t.err),
              onPressed: () => StoreScope.read(context).dispatch(ProcessStopRequested(url, p.terminal)),
            ),
          ]),
      ]),
    );
  }
}
