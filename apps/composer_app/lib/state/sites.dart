// Site-related presentation and live-view helpers.

import 'package:composer_protocol/composer_protocol.dart';

/// Where the live view of the application is on this server, from `/api/info`: a path to append to the server's
/// URL, or null: no live view here.
String? viewerPath(ServerInfo? info) {
  final path = info?.viewer;
  return path == null || path.isEmpty ? null : path;
}

/// The address of the live view: the server's URL, the viewer's path and the bare page. No token:
/// the webview authenticates with a browser session in its cookie (`setWebSessionCookie`).
String liveViewUrl(String workspaceUrl, String viewer) {
  final path = viewer.startsWith('/') ? viewer : '/$viewer';
  return Uri.parse('$workspaceUrl$path').replace(queryParameters: {'bare': '1'}).toString();
}

/// What the live view is labelled with for a session in [site]: the server has a viewer and the
/// session is in a site. Null: no live view. The view shows whatever the viewer has in front.
String? liveViewApp({required ServerInfo? info, required SiteSummary? site}) {
  if (viewerPath(info) == null || site == null) return null;
  return site.name;
}

/// `working · step 6`, `idle`: the one line of state under a session's title.
String stateLine(String activity, int steps) => switch (activity) {
      'running' => steps > 0 ? 'working · step $steps' : 'working',
      'waiting' => 'waiting for a slot',
      'approval' => 'needs approval',
      _ => activity,
    };
