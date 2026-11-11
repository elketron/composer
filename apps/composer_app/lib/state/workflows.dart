// Workflows: sites with an application and/or MCP servers (docs/workflows.md).

import 'package:composer_protocol/composer_protocol.dart';

/// A site is a workflow when its `index.md` sets `app:` or `mcp:`.
bool isWorkflow(SiteSummary site) => (site.app ?? '').isNotEmpty || site.mcp.isNotEmpty;

/// The workflows among the workspace's sites, in the order the server lists them.
List<SiteSummary> workflowsOf(List<SiteSummary> sites) => sites.where(isWorkflow).toList();

/// Where the live view of the application is on this server, from `/api/info`: a path to append to the workspace
/// URL, or null: no live view here.
String? viewerPath(ServerInfo? info) {
  final path = info?.viewer;
  return path == null || path.isEmpty ? null : path;
}

/// The address of the live view: the workspace URL, the viewer's path and the bare page. No token:
/// the webview authenticates with a browser session in its cookie (`setWebSessionCookie`).
String liveViewUrl(String workspaceUrl, String viewer) {
  final path = viewer.startsWith('/') ? viewer : '/$viewer';
  return Uri.parse('$workspaceUrl$path').replace(queryParameters: {'bare': '1'}).toString();
}

/// The application the live view shows for a session in [site]: the server has a viewer and the
/// site is a workflow with an application. Null: no live view.
String? liveViewApp({required ServerInfo? info, required SiteSummary? site}) {
  if (viewerPath(info) == null || site == null) return null;
  final app = site.app;
  return app == null || app.isEmpty ? null : app;
}

/// `working · step 6`, `idle`: the one line of state under a session's title.
String stateLine(String activity, int steps) => switch (activity) {
      'running' => steps > 0 ? 'working · step $steps' : 'working',
      'waiting' => 'waiting for a slot',
      'approval' => 'needs approval',
      _ => activity,
    };
