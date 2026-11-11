import 'package:flutter/foundation.dart';

import '../../state/frame_size.dart';
import '../../state/live_view_size.dart';

/// What the session screen asks of the live view's builder: the page to show, the size (which
/// chooses the page's fit) and where to report what the page and the load tell.
class LiveViewRequest {
  const LiveViewRequest({
    required this.address,
    required this.workspace,
    required this.token,
    required this.app,
    required this.size,
    required this.onFrameSize,
    required this.onLive,
  });

  /// The bare viewer page (no token in it).
  final String address;

  /// The workspace's URL, and the device's token: what asks for the browser session the page
  /// loads with.
  final String workspace;
  final String token;

  /// The application's name.
  final String app;
  final LiveViewSize size;

  /// The page reported the application's screen size.
  final ValueChanged<FrameSize> onFrameSize;

  /// False when the page cannot load, true again once it does.
  final ValueChanged<bool> onLive;
}
