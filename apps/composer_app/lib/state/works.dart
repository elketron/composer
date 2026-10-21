import 'package:composer_protocol/composer_protocol.dart';

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
