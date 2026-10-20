import 'package:flutter/material.dart';

import '../../state/chat_items.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../pages/page_navigation.dart';

/// A turn's tool calls and thoughts as one line (`6 steps · read 1 page, wrote 0001.md`);
/// tap to list them, tap a step for its arguments and result.
class StepsView extends StatefulWidget {
  const StepsView({super.key, required this.row, required this.url});

  final StepsRow row;

  /// The server.
  final String url;

  @override
  State<StepsView> createState() => _StepsViewState();
}

class _StepsViewState extends State<StepsView> {
  bool _open = false;
  int? _detail;

  @override
  Widget build(BuildContext context) {
    final steps = widget.row.steps;
    final tools = steps.whereType<ToolItem>().length;
    final running = steps.any((s) => s is ToolItem && s.result == null);
    return Container(
      decoration: BoxDecoration(color: t.surface, border: Border.all(color: t.border), borderRadius: BorderRadius.circular(t.radius)),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        InkWell(
          borderRadius: BorderRadius.circular(t.radius),
          onTap: () => setState(() => _open = !_open),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 7),
            child: Row(children: [
              Text(_open ? '▾' : '▸', style: mono(size: 10, color: t.faint)),
              const SizedBox(width: 8),
              Expanded(
                child: Text.rich(
                  TextSpan(style: const TextStyle(fontSize: 12.5, color: t.muted), children: [
                    TextSpan(
                      text: tools == 0 ? 'thought' : '$tools step${tools == 1 ? '' : 's'}',
                      style: const TextStyle(color: t.text, fontWeight: FontWeight.w500),
                    ),
                    TextSpan(text: ' · ${widget.row.summary}'),
                  ]),
                  maxLines: _open ? 3 : 1,
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              if (running) ...[
                const SizedBox(width: 8),
                const SizedBox(width: 10, height: 10, child: CircularProgressIndicator(strokeWidth: 1.5, color: t.warn)),
              ],
            ]),
          ),
        ),
        if (_open)
          Container(
            decoration: const BoxDecoration(border: Border(top: BorderSide(color: t.border))),
            padding: const EdgeInsets.fromLTRB(10, 6, 10, 8),
            child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
              for (final (i, step) in steps.indexed) ...[
                _step(context, i, step),
                if (_detail == i) _body(step),
              ],
            ]),
          ),
      ]),
    );
  }

  Widget _step(BuildContext context, int i, ChatItem step) {
    final nested = step is ToolItem && step.nested;
    final (name, arg, mark) = switch (step) {
      ToolItem(:final name, :final summary, :final result, :final isError) => (
          name,
          summary,
          result == null
              ? Text('…', style: mono(size: 12, color: t.warn))
              : isError
                  ? Text('✗', style: mono(size: 12, color: t.err))
                  : Text('✓', style: mono(size: 12, color: t.ok)),
        ),
      ThinkingItem(:final text) => ('thought', text.trim().split('\n').first, null),
      _ => ('', '', null),
    };
    final page = arg.startsWith('site://') ? arg : null;
    return InkWell(
      onTap: () => setState(() => _detail = _detail == i ? null : i),
      child: Padding(
        // A sub-agent's calls sit under the call that started it.
        padding: EdgeInsets.fromLTRB(nested ? 16 : 0, 3, 0, 3),
        child: Row(children: [
          if (nested) Text('↳ ', style: mono(size: 12, color: t.faint)),
          SizedBox(width: nested ? 80 : 96, child: Text(name, style: mono(size: 12, color: step is ThinkingItem ? t.faint : t.info))),
          Expanded(
            child: page == null
                ? Text(arg, maxLines: 1, overflow: TextOverflow.ellipsis, style: mono(size: 12))
                : GestureDetector(
                    onTap: () => openPage(context, widget.url, page),
                    child: Text(page.replaceFirst('site://', ''), maxLines: 1, overflow: TextOverflow.ellipsis, style: mono(size: 12, color: t.accent)),
                  ),
          ),
          if (mark != null) ...[const SizedBox(width: 8), mark],
        ]),
      ),
    );
  }

  Widget _body(ChatItem step) {
    final text = switch (step) {
      ToolItem(:final summary, :final detail, :final result) =>
        [summary, if (detail != null && detail.isNotEmpty) detail, result ?? '(running)'].join('\n\n'),
      ThinkingItem(:final text) => text,
      _ => '',
    };
    return Container(
      margin: const EdgeInsets.only(top: 2, bottom: 6),
      padding: const EdgeInsets.all(8),
      constraints: const BoxConstraints(maxHeight: 280),
      decoration: BoxDecoration(color: t.bg, border: Border.all(color: t.border), borderRadius: BorderRadius.circular(t.radius)),
      child: SingleChildScrollView(child: SelectableText(text.trim(), style: mono(size: 12, color: t.muted))),
    );
  }
}
