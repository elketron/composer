import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/apps.dart';
import '../../state/server_mark.dart';
import '../../theme/tokens.dart' as t;
import '../server_badge.dart';
import '../store_scope.dart';
import 'app_view.dart';

/// Builds the app's page; the webview unless a test puts something else.
typedef AppViewBuilder = Widget Function(BuildContext context, AppViewRequest request);

/// What the screen asks of the builder.
class AppViewRequest {
  const AppViewRequest({required this.workspace, required this.token, required this.name, required this.title, required this.path, required this.reloads});
  final String workspace;
  final String token;
  final String name;
  final String title;
  final String path;
  final Listenable reloads;
}

const _appRoute = 'app';

/// Opens app [name] of the server's open session: on its desk (when it is not there yet) and as
/// its own screen.
void openApp(BuildContext context, String url, String name, {AppViewBuilder? viewBuilder}) {
  StoreScope.read(context).dispatch(AppOpened(url, name));
  final settings = RouteSettings(name: _appRoute, arguments: name);
  final current = ModalRoute.of(context)?.settings;
  if (current?.name == _appRoute && current?.arguments == name) return;
  Navigator.of(context).push(
    MaterialPageRoute<void>(settings: settings, builder: (_) => AppScreen(url: url, name: name, viewBuilder: viewBuilder)),
  );
}

/// An app tab as its own screen (phones): its title, Reload, and the app edge to edge. It shows
/// the app at its tab's path, and follows the tab when the agent moves it.
class AppScreen extends StatefulWidget {
  const AppScreen({super.key, required this.url, required this.name, this.viewBuilder});

  final String url;
  final String name;
  final AppViewBuilder? viewBuilder;

  static Widget webview(BuildContext context, AppViewRequest r) =>
      AppView(workspace: r.workspace, token: r.token, name: r.name, title: r.title, path: r.path, reloads: r.reloads);

  @override
  State<AppScreen> createState() => _AppScreenState();
}

class _AppScreenState extends State<AppScreen> {
  final _reloads = _Reloads();

  @override
  void dispose() {
    _reloads.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final store = StoreScope.of(context);
    final status = store.state.statusOf(widget.url);
    final token = store.state.serverFor(widget.url)?.token;
    final mark = ServerMark.of(widget.url, info: status.info, entry: store.state.serverFor(widget.url));
    final title = appTitle(status.info, widget.name);
    final desk = status.chat?.desk;
    final path = (desk == null ? null : appTab(desk, widget.name)?.path) ?? '';
    return Scaffold(
      appBar: AppBar(
        toolbarHeight: 48,
        titleSpacing: 0,
        leadingWidth: 48,
        title: Text(title, key: const Key('app-title'), maxLines: 1, overflow: TextOverflow.ellipsis, style: const TextStyle(fontSize: 14, color: t.text)),
        actions: [
          IconButton(
            key: const Key('app-reload-bar'),
            tooltip: 'Reload',
            icon: const Icon(Icons.refresh, size: 20, color: t.muted),
            onPressed: _reloads.fire,
          ),
        ],
        bottom: PreferredSize(preferredSize: const Size.fromHeight(2), child: ServerStrip(mark: mark)),
      ),
      body: token == null
          ? const Center(child: Text('not paired with this server'))
          : (widget.viewBuilder ?? AppScreen.webview)(
              context,
              AppViewRequest(workspace: widget.url, token: token, name: widget.name, title: title, path: path, reloads: _reloads),
            ),
    );
  }
}

/// Reload, pressed.
class _Reloads extends ChangeNotifier {
  void fire() => notifyListeners();
}
