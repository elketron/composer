import 'package:flutter/material.dart';

import '../../state/actions.dart';
import '../../state/frame_size.dart';
import '../../state/live_view_room.dart';
import '../../state/live_view_size.dart';
import '../../state/workflows.dart';
import '../../theme/composer_theme.dart';
import '../../theme/tokens.dart' as t;
import '../store_scope.dart';
import 'live_view.dart';
import 'live_view_handle.dart';
import 'live_view_request.dart';
import 'page_cover.dart';

/// Builds what shows in the box (the webview); replaced in tests.
typedef LiveViewBuilder = Widget Function(BuildContext context, LiveViewRequest request);

/// The live view of the session's application with its handle under it. Created only when the
/// screen decides there is something to show (a viewer on the server, a workflow with an
/// application); while the app is in the background or another page covers the screen, the
/// webview is not kept: a placeholder of the same height stands in, and the page loads again
/// when the screen comes back.
class LiveViewSection extends StatefulWidget {
  const LiveViewSection({
    super.key,
    required this.url,
    required this.app,
    required this.viewer,
    required this.token,
    required this.maxHeight,
    this.viewBuilder = webview,
  });

  final String url;
  final String app;

  /// The viewer's path (`/view`) on this workspace.
  final String viewer;
  final String token;

  /// The most the view may take of the screen (the chat keeps the rest).
  final double maxHeight;
  final LiveViewBuilder viewBuilder;

  static Widget webview(BuildContext context, LiveViewRequest request) =>
      LiveView(
          address: request.address,
          workspace: request.workspace,
          token: request.token,
          size: request.size,
          onFrameSize: request.onFrameSize,
          onLive: request.onLive);

  @override
  State<LiveViewSection> createState() => _LiveViewSectionState();
}

class _LiveViewSectionState extends State<LiveViewSection> {
  /// The height while a finger drags the handle; null otherwise.
  double? _drag;

  /// False once the page could not load: the handle's dot goes out.
  bool _live = true;

  void _resize(LiveViewSize size) => StoreScope.read(context).dispatch(LiveViewResized(size));

  void _dragEnd(LiveViewRoom room) {
    final dragged = _drag;
    setState(() => _drag = null);
    if (dragged != null) _resize(room.snap(dragged));
  }

  @override
  Widget build(BuildContext context) {
    final state = StoreScope.of(context).state;
    final size = state.liveViewSize;
    final cover = PageCoverScope.maybeOf(context);
    final route = ModalRoute.of(context);
    return ListenableBuilder(
      listenable: cover?.top ?? Listenable.merge(const []),
      builder: (context, _) => LayoutBuilder(builder: (context, box) {
        final covered = cover != null && cover.top.value != null && cover.top.value != route;
        final shown = state.foreground && !covered;
        final room = LiveViewRoom(width: box.maxWidth, frame: state.liveFrame ?? FrameSize.assumed, maxHeight: widget.maxHeight);
        final height = _drag == null ? room.heightOf(size) : room.clampDrag(_drag!);
        final request = LiveViewRequest(
          address: liveViewUrl(widget.url, widget.viewer),
          workspace: widget.url,
          token: widget.token,
          app: widget.app,
          size: size,
          onFrameSize: (frame) => StoreScope.read(context).dispatch(LiveViewFrameSized(frame)),
          onLive: (live) {
            if (mounted && live != _live) setState(() => _live = live);
          },
        );
        return Column(mainAxisSize: MainAxisSize.min, children: [
          SizedBox(
            key: const Key('live-view'),
            height: height,
            child: shown
                ? widget.viewBuilder(context, request)
                : ColoredBox(
                    key: const Key('live-view-paused'),
                    color: t.surface,
                    child: Center(child: Text('live view paused', style: mono(size: 11, color: t.faint))),
                  ),
          ),
          LiveViewHandle(
            app: widget.app,
            live: _live,
            size: size,
            onTap: () => StoreScope.read(context).dispatch(const LiveViewStepped()),
            onDragStart: () => setState(() => _drag = height),
            onDragUpdate: (dy) => setState(() => _drag = room.clampDrag((_drag ?? height) + dy)),
            onDragEnd: () => _dragEnd(room),
            onGrow: size == LiveViewSize.large ? null : () => _resize(LiveViewSize.values[size.index + 1]),
            onShrink: size == LiveViewSize.small ? null : () => _resize(LiveViewSize.values[size.index - 1]),
          ),
        ]);
      }),
    );
  }
}
