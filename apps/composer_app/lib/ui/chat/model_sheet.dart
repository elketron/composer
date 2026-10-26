import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';

/// Pick the model the open session's turns use, from what the provider lists (OpenRouter: a few
/// hundred, so there is a filter), or go back to the server's.
void showModelSheet(BuildContext context, String url) {
  StoreScope.read(context).dispatch(ModelsRequested(url));
  showModalBottomSheet<void>(
    context: context,
    showDragHandle: true,
    isScrollControlled: true,
    builder: (sheet) => ModelSheet(url: url),
  );
}

class ModelSheet extends StatefulWidget {
  const ModelSheet({super.key, required this.url});

  final String url;

  @override
  State<ModelSheet> createState() => _ModelSheetState();
}

class _ModelSheetState extends State<ModelSheet> {
  String _filter = '';

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.of(context);
    final status = store.state.statusOf(widget.url);
    final chat = status.chat;
    final current = chat == null ? null : status.session(chat.session)?.model;
    final listed = status.models;
    final needle = _filter.toLowerCase();
    final models = (listed?.models ?? const <ModelInfo>[])
        .where((m) => m.id.toLowerCase().contains(needle) || (m.name?.toLowerCase().contains(needle) ?? false))
        .toList();
    void choose(String? model) {
      Navigator.pop(context);
      store.dispatch(ModelChosen(widget.url, model));
    }

    return SafeArea(
      child: Padding(
        padding: EdgeInsets.only(bottom: MediaQuery.viewInsetsOf(context).bottom),
        child: ConstrainedBox(
          constraints: BoxConstraints(maxHeight: MediaQuery.sizeOf(context).height * 0.75),
          child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            const Padding(
              padding: EdgeInsets.fromLTRB(16, 0, 16, 8),
              child: Text('Model', style: TextStyle(fontSize: 16, fontWeight: FontWeight.w600)),
            ),
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 0, 16, 4),
              child: TextField(
                key: const Key('model-filter'),
                autofocus: false,
                decoration: const InputDecoration(isDense: true, hintText: 'Filter by id or name', prefixIcon: Icon(Icons.search, size: 18)),
                onChanged: (text) => setState(() => _filter = text.trim()),
              ),
            ),
            Flexible(
              child: ListView(shrinkWrap: true, padding: const EdgeInsets.fromLTRB(8, 0, 8, 12), children: [
                if (listed == null)
                  const Padding(
                    padding: EdgeInsets.all(16),
                    child: Center(child: SizedBox(width: 18, height: 18, child: CircularProgressIndicator(strokeWidth: 2))),
                  ),
                if (listed != null && _filter.isEmpty)
                  ListTile(
                    key: const Key('server-model'),
                    selected: current == null,
                    leading: const Icon(Icons.dns_outlined, size: 20),
                    title: const Text("The server's"),
                    subtitle: Text(listed.defaultModel, style: mono(size: 11, color: t.faint)),
                    onTap: () => choose(null),
                  ),
                for (final m in models)
                  ListTile(
                    key: Key('model-${m.id}'),
                    dense: true,
                    selected: current == m.id,
                    title: Text(m.name ?? m.id, style: mono(size: 13, color: current == m.id ? t.accent : t.text)),
                    subtitle: Text(
                      [if (m.name != null) m.id, if (m.context != null) '${m.context! ~/ 1000}k context'].join(' · '),
                      style: mono(size: 11, color: t.faint),
                    ),
                    onTap: () => choose(m.id),
                  ),
                if (listed != null && models.isEmpty)
                  const Padding(
                    padding: EdgeInsets.all(16),
                    child: Text('No models listed', style: TextStyle(color: t.faint)),
                  ),
              ]),
            ),
          ]),
        ),
      ),
    );
  }
}
