import 'dart:ui';

/// A workspace's colour: `#rrggbb` from the server (its `[desktop] color`, else one the server
/// derived from its name).
///
/// The derivation is the server's (`composer_protocol::WorkspaceColor`), so an older server that
/// leaves the colour out still gets the same one the desktop shows.
class WorkspaceColor {
  const WorkspaceColor(this.value);

  /// `0xRRGGBB`.
  final int value;

  /// `#rrggbb` (the `#` optional); `null` for anything else.
  static WorkspaceColor? parse(String? text) {
    if (text == null) return null;
    final hex = text.trim().replaceFirst(RegExp(r'^#'), '');
    if (hex.length != 6 || !RegExp(r'^[0-9a-fA-F]{6}$').hasMatch(hex)) return null;
    return WorkspaceColor(int.parse(hex, radix: 16));
  }

  /// A distinct, readable-on-dark hue from [name]: fixed saturation and lightness.
  static WorkspaceColor fromName(String name) {
    final hash = name.codeUnits.fold<int>(2166136261, (h, b) => ((h ^ b) * 16777619) & 0xffffffff);
    return WorkspaceColor(_hslToRgb((hash % 360).toDouble(), 0.55, 0.62));
  }

  Color get color => Color(0xff000000 | value);

  String get hex => '#${value.toRadixString(16).padLeft(6, '0')}';

  @override
  bool operator ==(Object other) => other is WorkspaceColor && other.value == value;

  @override
  int get hashCode => value.hashCode;

  static int _hslToRgb(double h, double s, double l) {
    final c = (1 - (2 * l - 1).abs()) * s;
    final x = c * (1 - ((h / 60) % 2 - 1).abs());
    final m = l - c / 2;
    final (r, g, b) = switch (h ~/ 60) {
      0 => (c, x, 0.0),
      1 => (x, c, 0.0),
      2 => (0.0, c, x),
      3 => (0.0, x, c),
      4 => (x, 0.0, c),
      _ => (c, 0.0, x),
    };
    int byte(double v) => ((v + m) * 255).round();
    return (byte(r) << 16) | (byte(g) << 8) | byte(b);
  }
}
