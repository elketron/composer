import 'package:composer_protocol/composer_protocol.dart';

import 'projects.dart';

/// Stage states that let the next stage start.
const finishedStages = {'done', 'approved'};

/// `site://coding/save-system/index.md` → `coding/save-system`, as `work.run` takes it.
String workSpec(String url) => url.replaceFirst('site://', '').replaceFirst(RegExp(r'/index\.md$'), '');

/// The stage `work.run` starts by default: the first that is stale or not finished.
StageState? nextStage(WorkSummary w) => w.stages.where((s) => s.stale || !finishedStages.contains(s.status)).firstOrNull;

/// Whether a work's next stage can start: one is left, none runs, and it doesn't wait for
/// review (unless what it read changed since).
bool runnable(WorkSummary w) {
  if (w.stages.any((s) => s.status == 'running')) return false;
  final next = nextStage(w);
  return next != null && (next.status != 'review' || next.stale);
}

/// `Run`, or `Rerun from 02` when the next stage is stale.
String runLabel(WorkSummary w) {
  final next = nextStage(w);
  return next != null && next.stale ? 'Rerun from ${next.name.split('_').first}' : 'Run';
}

/// The stage of a work that waits for you at its gate, if one does.
StageState? waitingStage(WorkSummary w) => w.stages.where((s) => s.status == 'review').firstOrNull;

/// `site://video/launch-video/index.md` → `video`.
String workSite(String url) => url.replaceFirst('site://', '').split('/').first;

/// Whether a work links the project `folder` (its `project:` is `~/…` or a path).
bool linksProject(WorkSummary w, String folder) =>
    w.project != null && (w.project == folder || projectName(w.project!) == projectName(folder));
