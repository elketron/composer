import 'package:composer_protocol/composer_protocol.dart';
import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/projects.dart';
import '../../state/works.dart';
import '../../state/server_status.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../chat/chat_menu.dart';
import '../chat/navigation.dart';
import '../chat/review_page.dart';
import '../format.dart';
import '../home/activity_dot.dart';
import '../home/section_label.dart';
import '../pages/page_navigation.dart';
import '../pipelines/new_work_dialog.dart';
import '../pipelines/stage_dots.dart';
import '../store_scope.dart';

/// What is going on in one project: its works across pipelines, branches waiting to merge, its
/// sessions, its memory page and its switches. The desktop's Project tab and the phone's project
/// screen.
class ProjectOverviewView extends StatefulWidget {
  const ProjectOverviewView({super.key, required this.url, required this.status, required this.folder, this.actions = true});

  final String url;
  final ServerStatus status;

  /// The project's folder.
  final String folder;

  /// New chat here and the like at the bottom (the phone screen has its own).
  final bool actions;

  @override
  State<ProjectOverviewView> createState() => _ProjectOverviewViewState();
}

class _ProjectOverviewViewState extends State<ProjectOverviewView> {
  @override
  void initState() {
    super.initState();
    // Fresh each time it is shown; the last one is shown meanwhile. After the frame: not while
    // the tree is building.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) StoreScope.read(context).dispatch(ProjectOverviewRequested(widget.url, widget.folder));
    });
  }

  @override
  void didUpdateWidget(ProjectOverviewView old) {
    super.didUpdateWidget(old);
    if (old.folder != widget.folder) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) StoreScope.read(context).dispatch(ProjectOverviewRequested(widget.url, widget.folder));
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.read(context);
    final url = widget.url;
    final status = widget.status;
    final o = status.overviews[widget.folder];
    final sessions = projectSessions(status, widget.folder);
    final top = sessions.where((s) => s.ticket == null).toList();
    // A toggle's reply updates `harnesses`; the overview's are from when it was asked.
    final switches = [
      for (final h in status.harnesses[widget.folder] ?? o?.harnesses ?? const <HarnessSwitch>[])
        if (h.allowed && !h.entry.contains(':')) h,
    ];

    Widget row({required List<Widget> children, VoidCallback? onTap, Key? key}) => InkWell(
          key: key,
          onTap: onTap,
          child: Container(
            padding: const EdgeInsets.symmetric(vertical: 7, horizontal: 2),
            decoration: const BoxDecoration(border: Border(bottom: BorderSide(color: t.border))),
            child: Row(children: children),
          ),
        );
    Widget chip(String text) => Container(
          padding: const EdgeInsets.symmetric(horizontal: 5),
          decoration: BoxDecoration(border: Border.all(color: t.borderStrong), borderRadius: BorderRadius.circular(t.radiusSm)),
          child: Text(text, style: mono(size: 10.5)),
        );

    return ListView(key: const Key('project-overview'), padding: const EdgeInsets.fromLTRB(16, 12, 16, 16), children: [
      Row(children: [
        Text(projectName(widget.folder), style: mono(size: 15, color: t.text, weight: FontWeight.w600)),
        const SizedBox(width: 8),
        Expanded(child: Text(o?.path ?? widget.folder, overflow: TextOverflow.ellipsis, style: mono(size: 11.5, color: t.faint))),
        if (o?.branch != null) ...[const SizedBox(width: 6), chip(o!.branch!)],
        if ((o?.uncommitted ?? 0) > 0) ...[
          const SizedBox(width: 6),
          Text('${o!.uncommitted} uncommitted', style: mono(size: 11, color: t.warn)),
        ],
        IconButton(
          tooltip: 'Reload',
          visualDensity: VisualDensity.compact,
          onPressed: () => store.dispatch(ProjectOverviewRequested(url, widget.folder)),
          icon: const Icon(Icons.refresh, size: 16, color: t.muted),
        ),
      ]),
      if (o == null) const Padding(padding: EdgeInsets.only(top: 24), child: Center(child: CircularProgressIndicator(strokeWidth: 2))),
      if (o != null && o.works.isNotEmpty) ...[
        const SectionLabel('Works'),
        for (final w in o.works)
          row(
            key: Key('work-${w.url}'),
            onTap: () => openPage(context, url, w.url),
            children: [
              chip(w.pipeline),
              const SizedBox(width: 8),
              if (w.stages.isNotEmpty) ...[StageDots(w.stages), const SizedBox(width: 6)],
              Expanded(child: Text(w.title, overflow: TextOverflow.ellipsis, style: mono(size: 12.5, color: t.text))),
              Text(w.status ?? '', style: mono(size: 11, color: t.muted)),
              if (runnable(w)) ...[
                const SizedBox(width: 8),
                InkWell(
                  key: Key('work-run-${w.url}'),
                  onTap: () => store.dispatch(WorkRunRequested(url, workSpec(w.url), project: widget.folder)),
                  child: Text(runLabel(w), style: mono(size: 11.5, color: t.accent)),
                ),
              ],
            ],
          ),
      ],
      if (o != null && o.branches.isNotEmpty) ...[
        const SectionLabel('Branches to merge'),
        for (final b in o.branches)
          row(
            key: Key('branch-${b.ticket}'),
            onTap: () => openReview(context, url, b.ticket),
            children: [
              Text(b.ticket, style: mono(size: 12, color: t.text)),
              const SizedBox(width: 8),
              Expanded(child: Text(b.title, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 12.5))),
              Text(
                switch (b.commits) { 0 => 'nothing to merge', 1 => '1 commit', final n => '$n commits' },
                style: mono(size: 11, color: t.faint),
              ),
              const SizedBox(width: 10),
              Text('Changes', style: mono(size: 11.5, color: t.accent)),
            ],
          ),
      ],
      const SectionLabel('Sessions'),
      if (top.isEmpty) const Text('No chats here yet.', style: TextStyle(fontSize: 12.5, color: t.faint)),
      for (final s in top) ...[
        row(
          key: Key('project-session-${s.id}'),
          onTap: () => openSession(context, url, s.id),
          children: [
            ActivityDot(status.activity(s), size: 6),
            const SizedBox(width: 8),
            Text(s.agent, style: mono(size: 12.5, color: t.text)),
            const SizedBox(width: 6),
            Text('#${s.id}', style: mono(size: 11, color: t.faint)),
            const SizedBox(width: 8),
            Expanded(
              child: Text((s.preview ?? '').replaceAll('\n', ' '), maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 12, color: t.muted)),
            ),
            Text(isBusy(status.activity(s)) ? status.activity(s) : ago(s.updated), style: mono(size: 11, color: t.faint)),
          ],
        ),
        for (final w in sessions.where((w) => w.parent == s.id))
          row(
            onTap: () => openSession(context, url, w.id),
            children: [
              const SizedBox(width: 14),
              ActivityDot(status.activity(w), size: 6),
              const SizedBox(width: 8),
              Text(w.ticket ?? '#${w.id}', style: mono(size: 12, color: t.text)),
              const SizedBox(width: 8),
              Expanded(
                child: Text((w.preview ?? '').replaceAll('\n', ' '), maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 12, color: t.muted)),
              ),
              Text(status.activity(w), style: mono(size: 11, color: activityColor(status.activity(w)))),
            ],
          ),
      ],
      if (o?.memory != null) ...[
        const SectionLabel('Memory'),
        InkWell(
          onTap: () => openPage(context, url, o.memory!.url),
          child: Container(
            padding: const EdgeInsets.only(left: 10),
            decoration: const BoxDecoration(border: Border(left: BorderSide(color: t.borderStrong, width: 2))),
            child: Text(o!.memory!.excerpt, style: const TextStyle(fontSize: 12.5, color: t.muted)),
          ),
        ),
      ],
      if (switches.isNotEmpty) ...[
        const SectionLabel('Switches'),
        for (final h in switches)
          Row(children: [
            Transform.scale(
              scale: 0.7,
              child: Switch(
                value: h.on,
                onChanged: (on) => store.dispatch(HarnessToggled(url, widget.folder, h.entry, on)),
              ),
            ),
            Text(harnessName(h.entry), style: const TextStyle(fontSize: 12.5)),
            const Spacer(),
            if ((o?.toolRules ?? 0) > 0) Text('${o!.toolRules} tool rule${o.toolRules == 1 ? '' : 's'}', style: mono(size: 11, color: t.faint)),
          ]),
      ],
      if (widget.actions) ...[
        const SizedBox(height: 16),
        Wrap(spacing: 8, runSpacing: 6, children: [
          FilledButton(
            key: const Key('project-new-chat'),
            onPressed: () {
              store.dispatch(NewSessionRequested(url, project: widget.folder));
              if (!isWide(context)) pushChat(context, url);
            },
            child: const Text('New chat here'),
          ),
          OutlinedButton(
            key: const Key('project-new-work'),
            onPressed: () => showNewWork(context, url, project: widget.folder),
            child: const Text('New work here'),
          ),
          OutlinedButton(
            onPressed: () => store.dispatch(ScopeChanged(url, status.scope == widget.folder ? null : widget.folder)),
            child: Text(status.scope == widget.folder ? 'Show all sessions' : 'Show only its sessions'),
          ),
        ]),
      ],
    ]);
  }
}
