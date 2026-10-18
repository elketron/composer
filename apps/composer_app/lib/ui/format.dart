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
