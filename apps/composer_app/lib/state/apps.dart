// The user's own local web servers (`[apps.<name>]`), which the server forwards at
// `/apps/<name>/` and a desk's `app` tab shows.

import 'package:composer_protocol/composer_protocol.dart';

/// A desk tab's id for app [name] (one per app per desk).
String appTabId(String name) => 'app:$name';

/// Where app [name] is served on the workspace: `<workspace>/apps/<name>/` (always with the slash,
/// so the app's relative URLs resolve under it). No token: the webview authenticates with a
/// browser session in its cookie (`setWebSessionCookie`).
Uri appRoot(String workspaceUrl, String name) {
  final base = Uri.parse(workspaceUrl);
  final path = base.path.endsWith('/') ? base.path.substring(0, base.path.length - 1) : base.path;
  // A new Uri: `replace(query: null)` would keep the workspace's query.
  return Uri(
    scheme: base.scheme,
    host: base.host,
    port: base.hasPort ? base.port : null,
    path: '$path/apps/${Uri.encodeComponent(name)}/',
  );
}

/// The address of app [name] at [path] (relative to its root; may carry `?query` and `#fragment`;
/// null or empty: its page).
String appUrl(String workspaceUrl, String name, String? path) {
  final root = appRoot(workspaceUrl, name).toString();
  var rest = path ?? '';
  while (rest.startsWith('/')) {
    rest = rest.substring(1);
  }
  return '$root$rest';
}

/// [target] is app [name]'s own (the same origin, under `/apps/<name>/`): it stays in the app's
/// webview. Anything else opens outside.
bool insideApp(String workspaceUrl, String name, Uri target) {
  final root = appRoot(workspaceUrl, name);
  if (target.scheme != root.scheme || target.host != root.host || target.port != root.port) return false;
  final bare = root.path.substring(0, root.path.length - 1);
  return target.path == bare || target.path.startsWith(root.path);
}

/// What app [name] is called: its title from the server's `apps`, else its name.
String appTitle(ServerInfo? info, String name) {
  final title = info?.apps.where((a) => a.name == name).firstOrNull?.title;
  return title == null || title.isEmpty ? name : title;
}

/// The app tabs on a desk, in its order.
List<(DeskEntry, DeskTabApp)> appTabs(Desk desk) => [
      for (final entry in desk.tabs)
        if (entry.tab case final DeskTabApp app) (entry, app),
    ];

/// App [name]'s tab on [desk], if it is there.
DeskTabApp? appTab(Desk desk, String name) => appTabs(desk).where((t) => t.$2.name == name).firstOrNull?.$2;
