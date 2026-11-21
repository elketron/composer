import 'dart:ui';

import 'package:composer_protocol/composer_protocol.dart';

import 'server_entry.dart';

/// A paired server as the app marks it: its name and a colour derived from that name, the same
/// pair everywhere, so you always know which server you are looking at.
class ServerMark {
  const ServerMark(this.name, this.color);

  final String name;
  final ServerColor color;

  /// Named as saved at pairing, else as `/api/info` names it, else by its host.
  factory ServerMark.of(String url, {ServerInfo? info, ServerEntry? entry}) {
    final name = entry?.name ?? info?.name ?? (Uri.tryParse(url)?.host ?? url);
    return ServerMark(name, ServerColor.fromName(name));
  }
}

/// A server's colour, derived from its name (the derivation composer's other clients use).
class ServerColor {
  const ServerColor(this.value);

  /// `0xRRGGBB`.
  final int value;

  /// A distinct, readable-on-dark hue from [name]: fixed saturation and lightness.
  static ServerColor fromName(String name) {
    final hash = name.codeUnits.fold<int>(2166136261, (h, b) => ((h ^ b) * 16777619) & 0xffffffff);
    return ServerColor(_hslToRgb((hash % 360).toDouble(), 0.55, 0.62));
  }

  Color get color => Color(0xff000000 | value);

  String get hex => '#${value.toRadixString(16).padLeft(6, '0')}';

  @override
  bool operator ==(Object other) => other is ServerColor && other.value == value;

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
