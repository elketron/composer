// Small text helpers for the UI.

import '../state/types.dart';

/// `2026-10-18T12:00:00Z` → `3m`, `2h`, `5d`, or the date.
String ago(String rfc3339, {DateTime? now}) {
  final time = DateTime.tryParse(rfc3339);
  if (time == null) return '';
  final d = (now ?? DateTime.now()).difference(time);
  if (d.inMinutes < 1) return 'now';
  if (d.inHours < 1) return '${d.inMinutes}m';
  if (d.inDays < 1) return '${d.inHours}h';
  if (d.inDays < 30) return '${d.inDays}d';
  return rfc3339.substring(0, 10);
}

String linkLabel(LinkState link) => switch (link) {
      LinkState.connecting => 'connecting',
      LinkState.online => 'online',
      LinkState.offline => 'offline',
    };

/// `https://home.tail1234.ts.net` → `home.tail1234.ts.net`.
String shortUrl(String url) => url.replaceFirst(RegExp(r'^https?://'), '');

final _processNotice = RegExp(r'^\[(?:automatic notice: )?background process \S+ \(`(.*?)`\) ([^\]\n]*)\]');

/// A session's last-message preview: the server's note that a background process ended
/// (`[automatic notice: background process t… (`cmd`) was stopped by the user]…`) reads as
/// `process stopped · cmd`, as in the chat; anything else as is.
String previewText(String preview) {
  final m = _processNotice.firstMatch(preview);
  if (m == null) return preview;
  final how = m.group(2)!.contains('stopped') ? 'stopped' : 'ended';
  return 'process $how · ${m.group(1)}';
}
