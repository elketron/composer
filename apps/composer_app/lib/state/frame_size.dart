/// The pixel size of the application's screen as the live view's page reports it.
class FrameSize {
  const FrameSize(this.width, this.height);

  /// What to assume until the page has reported a frame: a 16:10 screen.
  static const assumed = FrameSize(1600, 1000);

  /// The smallest and largest side taken from the page (pixels); the page is not trusted.
  static const minSide = 1.0;
  static const maxSide = 16384.0;

  /// The size for numbers from the page, or null when they are not a size: two finite numbers,
  /// each from [minSide] to [maxSide].
  static FrameSize? tryFrom(Object? width, Object? height) {
    bool side(num n) => n.isFinite && n >= minSide && n <= maxSide;
    if (width is! num || height is! num || !side(width) || !side(height)) return null;
    return FrameSize(width.toDouble(), height.toDouble());
  }

  final double width;
  final double height;

  double get aspect => width / height;

  @override
  bool operator ==(Object other) => other is FrameSize && other.width == width && other.height == height;

  @override
  int get hashCode => Object.hash(width, height);

  @override
  String toString() => '${width.round()}x${height.round()}';
}
