import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';
import 'chat_menu.dart';

/// Claude Code for the open chat's project, in its header: shown when the project allows it,
/// a click turns it on or off there (`harness.set`).
class HarnessChip extends StatefulWidget {
  const HarnessChip({super.key, required this.url, required this.project, this.entry = 'claude-code'});

  final String url;
  final String project;
  final String entry;

  @override
  State<HarnessChip> createState() => _HarnessChipState();
}

class _HarnessChipState extends State<HarnessChip> {
  @override
  void initState() {
    super.initState();
    _ask();
  }

  @override
  void didUpdateWidget(HarnessChip old) {
    super.didUpdateWidget(old);
    if (old.project != widget.project) _ask();
  }

  /// What the project allows (once per project; the reply is kept in the status).
  void _ask() {
    final store = StoreScope.read(context);
    if (!store.state.statusOf(widget.url).harnesses.containsKey(widget.project)) {
      store.dispatch(HarnessesRequested(widget.url, widget.project));
    }
  }

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.of(context);
    final h = store.state.statusOf(widget.url).harnesses[widget.project]?.where((h) => h.entry == widget.entry && h.allowed).firstOrNull;
    if (h == null) return const SizedBox.shrink();
    final name = harnessName(widget.entry);
    return Tooltip(
      message: h.on ? '$name works tickets here: click to turn it off' : '$name is off here: click to turn it on',
      child: InkWell(
        key: Key('harness-chip-${widget.entry}'),
        onTap: () => store.dispatch(HarnessToggled(widget.url, widget.project, widget.entry, !h.on)),
        borderRadius: BorderRadius.circular(t.radiusSm),
        child: Container(
          padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 1),
          decoration: BoxDecoration(
            border: Border.all(color: (h.on ? t.info : t.borderStrong).withValues(alpha: h.on ? 0.5 : 1)),
            borderRadius: BorderRadius.circular(t.radiusSm),
          ),
          child: Text('${widget.entry} ${h.on ? 'on' : 'off'}', style: mono(size: 11, color: h.on ? t.info : t.faint)),
        ),
      ),
    );
  }
}
