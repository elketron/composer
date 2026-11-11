/// The three sizes of the live view on the session screen. The handle under the view drags between
/// them and a tap steps to the next one; [LiveViewRoom] says how tall each is. Only the name is
/// kept on the device.
enum LiveViewSize {
  /// A short peek: the whole screen inside a thumbnail.
  small,

  /// The whole screen at the phone's width, no bands.
  medium,

  /// As tall as the layout allows, the screen filling that height (wider than the phone: pan it).
  large;

  /// The next size, wrapping from large back to small (a tap on the handle).
  LiveViewSize get next => values[(index + 1) % values.length];

  /// How the page fits the screen into the view: [large] fills the height, the rest show it whole.
  String get fit => this == large ? 'fill-height' : 'contain';

  /// The size stored under [name], if it is one.
  static LiveViewSize? parse(String? name) => values.where((s) => s.name == name).firstOrNull;
}
