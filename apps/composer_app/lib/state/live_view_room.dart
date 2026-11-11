import 'dart:math';

import 'frame_size.dart';
import 'live_view_size.dart';

/// What the live view has to work with: the phone's width, the application's screen shape and the
/// most height the layout can give (the chat keeps the rest). Decides each size's height.
class LiveViewRoom {
  const LiveViewRoom({required this.width, required this.frame, required this.maxHeight});

  /// The peek's height (logical px).
  static const peek = 96.0;

  final double width;

  /// The screen's size; [FrameSize.assumed] until the page reports it.
  final FrameSize frame;
  final double maxHeight;

  /// The tallest the view gets: the room left, but never less than the peek.
  double get tallest => max(maxHeight, peek);

  double heightOf(LiveViewSize size) => switch (size) {
        LiveViewSize.small => peek,
        LiveViewSize.medium => min(max(width / frame.aspect, peek), tallest),
        LiveViewSize.large => tallest,
      };

  /// A drag's height kept between the peek and the tallest.
  double clampDrag(double height) => height.clamp(peek, tallest);

  /// The size whose height is nearest to [height] (the end of a drag).
  LiveViewSize snap(double height) =>
      LiveViewSize.values.reduce((a, b) => (heightOf(a) - height).abs() <= (heightOf(b) - height).abs() ? a : b);
}
