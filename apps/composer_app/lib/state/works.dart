import 'package:composer_protocol/composer_protocol.dart';

import 'projects.dart';

/// Stage states that let the next stage start.
const finishedStages = {'done', 'approved'};

/// `site://coding/save-system/index.md` → `coding/save-system`, as `work.run` takes it.
String workSpec(String url) => url.replaceFirst('site://', '').replaceFirst(RegExp(r'/index\.md$'), '');

/// Whether a work's next stage can start: one is left, none runs, and it doesn't wait for review.
bool runnable(WorkSummary w) {
  if (w.stages.any((s) => s.status == 'running')) return false;
  final next = w.stages.where((s) => !finishedStages.contains(s.status)).firstOrNull;
  return next != null && next.status != 'review';
}

/// The stage of a work that waits for you at its gate, if one does.
StageState? waitingStage(WorkSummary w) => w.stages.where((s) => s.status == 'review').firstOrNull;

/// `site://video/launch-video/index.md` → `video`.
String workSite(String url) => url.replaceFirst('site://', '').split('/').first;

/// Whether a work links the project `folder` (its `project:` is `~/…` or a path).
bool linksProject(WorkSummary w, String folder) =>
    w.project != null && (w.project == folder || projectName(w.project!) == projectName(folder));
