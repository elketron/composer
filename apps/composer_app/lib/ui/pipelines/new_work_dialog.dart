import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../state/actions.dart';
import '../../state/projects.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../pages/page_navigation.dart';
import '../projects/project_picker.dart';
import '../store_scope.dart';

/// Opens New work for a server; `project` (a folder) starts chosen.
void showNewWork(BuildContext context, String url, {String? project}) => showDialog<void>(
      context: context,
      builder: (_) => Dialog(
        alignment: const Alignment(0, -0.5),
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 640),
          child: NewWorkDialog(url: url, parent: context, project: project),
        ),
      ),
    );

/// A piece of work in a pipeline: which pipeline (or a template to make one from), its folder's
/// name, the project it links, the brief. The pipeline's stages show before you create it.
class NewWorkDialog extends StatefulWidget {
  const NewWorkDialog({super.key, required this.url, required this.parent, this.project});

  final String url;

  /// Where the work's page opens (the dialog's own context goes away with it).
  final BuildContext parent;
  final String? project;

  @override
  State<NewWorkDialog> createState() => _NewWorkDialogState();
}

class _NewWorkDialogState extends State<NewWorkDialog> {
  final _name = TextEditingController();
  final _brief = TextEditingController();

  /// The chosen pipeline's site, or a template's kind.
  String? _choice;
  late String? _project = widget.project;
  bool _done = false;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) return;
      final store = StoreScope.read(context);
      store.dispatch(NewWorkReset(widget.url));
      store.dispatch(PipelinesRequested(widget.url));
    });
  }

  @override
  void dispose() {
    _name.dispose();
    _brief.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.of(context);
    final status = store.state.statusOf(widget.url);
    final catalog = status.pipelines;
    final pipelines = catalog?.pipelines ?? const <PipelineSummary>[];
    // Templates of kinds there is no pipeline for yet.
    final templates = [
      for (final t in catalog?.templates ?? const <PipelineSummary>[])
        if (!pipelines.any((p) => p.site == t.site)) t,
    ];
    final choice = _choice ?? pipelines.firstOrNull?.site ?? templates.firstOrNull?.site;
    final pipeline = pipelines.where((p) => p.site == choice).firstOrNull;
    final template = pipeline == null ? templates.where((t) => t.site == choice).firstOrNull : null;
    final chosen = pipeline ?? template;
    final work = status.newWork;
    final name = _name.text.trim();
    final ready = chosen != null && name.isNotEmpty && !work.busy && (!chosen.needsProject || _project != null);

    // Made: open its page.
    if (work.created != null && !_done) {
      _done = true;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!mounted) return;
        Navigator.pop(context);
        openPage(widget.parent, widget.url, work.created!);
        StoreScope.read(widget.parent).dispatch(NewWorkReset(widget.url));
      });
    }

    void create() {
      if (!ready) return;
      store.dispatch(WorkCreateRequested(
        widget.url,
        pipeline: chosen.site,
        name: name,
        project: _project,
        brief: _brief.text.trim().isEmpty ? null : _brief.text.trim(),
        template: template?.site,
      ));
    }

    Widget label(String text, [String? hint]) => Padding(
          padding: const EdgeInsets.only(top: 14, bottom: 7),
          child: Row(children: [
            Text(text.toUpperCase(), style: mono(size: 10.5, color: t.faint)),
            if (hint != null) ...[const SizedBox(width: 8), Expanded(child: Text(hint, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 11.5, color: t.faint)))],
          ]),
        );

    Widget option(PipelineSummary p, {required bool fromTemplate}) {
      final on = p.site == choice;
      return Material(
        color: on ? t.accent.withValues(alpha: 0.12) : Colors.transparent,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(t.radius),
          side: BorderSide(color: on ? t.accent.withValues(alpha: 0.55) : t.border),
        ),
        child: InkWell(
          key: Key('pipeline-${p.site}'),
          borderRadius: BorderRadius.circular(t.radius),
          onTap: () => setState(() => _choice = p.site),
          child: Padding(
            padding: const EdgeInsets.all(10),
            child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
              Icon(on ? Icons.radio_button_checked : Icons.radio_button_off, size: 15, color: on ? t.accent : t.borderStrong),
              const SizedBox(width: 8),
              Expanded(
                child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                  Row(children: [
                    Flexible(child: Text(p.site, overflow: TextOverflow.ellipsis, style: mono(size: 13, color: t.text, weight: FontWeight.w500))),
                    const SizedBox(width: 6),
                    Text(fromTemplate ? 'template' : '${p.works} work${p.works == 1 ? '' : 's'}', style: mono(size: 10.5, color: t.faint)),
                  ]),
                  const SizedBox(height: 2),
                  Text(
                    p.stages.map((s) => s.title.toLowerCase()).join(' → '),
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: const TextStyle(fontSize: 11.5, color: t.muted),
                  ),
                ]),
              ),
            ]),
          ),
        ),
      );
    }

    Widget projectChip(String? folder, {String? label}) {
      final on = folder == _project && label == null;
      return ChoiceChip(
        key: Key('work-project-${label ?? (folder == null ? 'none' : projectName(folder))}'),
        label: Text(label ?? (folder == null ? 'none' : projectName(folder)), style: mono(size: 12, color: on ? t.text : t.muted)),
        selected: on,
        showCheckmark: false,
        selectedColor: t.accent.withValues(alpha: 0.12),
        backgroundColor: t.surface,
        side: BorderSide(color: on ? t.accent : t.borderStrong),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(t.radius)),
        onSelected: (_) {
          if (label != null) {
            showProjectPicker(context, widget.url, (p) => setState(() => _project = p.folder ?? p.path), selected: _project);
          } else {
            setState(() => _project = folder);
          }
        },
      );
    }

    final recent = [
      for (final p in status.projects.take(5))
        if ((p.folder ?? p.path) != _project) p.folder ?? p.path,
    ].take(4);

    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.enter, control: true): create,
      },
      child: SingleChildScrollView(
        padding: const EdgeInsets.fromLTRB(18, 16, 18, 16),
        child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Text('New work on ${status.info?.name ?? 'this server'}', style: const TextStyle(fontSize: 16, fontWeight: FontWeight.w600)),
          label('Pipeline', 'a site per kind of work'),
          if (catalog == null)
            const Padding(padding: EdgeInsets.all(12), child: Center(child: CircularProgressIndicator(strokeWidth: 2)))
          else if (pipelines.isEmpty && templates.isEmpty)
            const Text('No pipelines and no templates: run composer update on the server.', style: TextStyle(fontSize: 12.5, color: t.faint))
          else
            LayoutBuilder(
              builder: (context, box) {
                final columns = box.maxWidth > 480 ? 3 : (box.maxWidth > 320 ? 2 : 1);
                final width = (box.maxWidth - 8 * (columns - 1)) / columns;
                return Wrap(spacing: 8, runSpacing: 8, children: [
                  for (final p in pipelines) SizedBox(width: width, child: option(p, fromTemplate: false)),
                  for (final p in templates) SizedBox(width: width, child: option(p, fromTemplate: true)),
                ]);
              },
            ),
          label('Name', chosen == null ? null : "the work's folder: ${chosen.site}/${name.isEmpty ? '<name>' : name}"),
          TextField(
            key: const Key('work-name'),
            controller: _name,
            autofocus: true,
            style: mono(size: 13, color: t.text),
            inputFormatters: [
              // A folder name: spaces become dashes, the rest is dropped.
              TextInputFormatter.withFunction((old, value) {
                final text = value.text.replaceAll(' ', '-').replaceAll(RegExp(r'[^A-Za-z0-9_-]'), '');
                return TextEditingValue(text: text, selection: TextSelection.collapsed(offset: text.length));
              }),
            ],
            decoration: const InputDecoration(isDense: true, hintText: 'save-system'),
            onChanged: (_) => setState(() {}),
          ),
          label('Project', chosen?.needsProject ?? false ? '${chosen!.site}: where the code lives' : 'optional: what it is about'),
          Wrap(spacing: 6, runSpacing: 6, children: [
            if (_project != null) projectChip(_project),
            for (final folder in recent) projectChip(folder),
            if (!(chosen?.needsProject ?? false)) projectChip(null),
            projectChip(null, label: 'other…'),
          ]),
          if (chosen != null && chosen.stages.isNotEmpty) ...[
            label('Stages'),
            Wrap(spacing: 6, runSpacing: 6, children: [
              for (final s in chosen.stages)
                Container(
                  padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 5),
                  decoration: BoxDecoration(border: Border.all(color: t.border), borderRadius: BorderRadius.circular(t.radius)),
                  child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
                    Text.rich(TextSpan(children: [
                      TextSpan(text: '${s.name.split('_').first} ', style: mono(size: 11, color: t.faint, weight: FontWeight.w600)),
                      TextSpan(text: s.title.toLowerCase(), style: mono(size: 12, color: t.text)),
                    ])),
                    Text(
                      [s.harness ?? 'composer', if (s.gate) 'gate', if (s.worktree) 'worktree'].join(' · '),
                      style: const TextStyle(fontSize: 10.5, color: t.faint),
                    ),
                  ]),
                ),
            ]),
          ],
          label('Brief', 'goes into the first stage'),
          TextField(
            key: const Key('work-brief'),
            controller: _brief,
            minLines: 3,
            maxLines: 8,
            style: const TextStyle(fontSize: 13),
            decoration: const InputDecoration(isDense: true, hintText: 'What this piece of work is for'),
          ),
          if (work.error != null) Padding(padding: const EdgeInsets.only(top: 10), child: Text(work.error!, style: const TextStyle(fontSize: 12.5, color: t.err))),
          const SizedBox(height: 14),
          Row(mainAxisAlignment: MainAxisAlignment.end, children: [
            TextButton(onPressed: () => Navigator.pop(context), child: const Text('Cancel')),
            const SizedBox(width: 8),
            FilledButton(
              key: const Key('work-create'),
              onPressed: ready ? create : null,
              child: Text(work.busy ? 'Creating…' : (template != null ? 'Make ${template.site} and create · ctrl-enter' : 'Create · ctrl-enter')),
            ),
          ]),
        ]),
      ),
    );
  }
}
