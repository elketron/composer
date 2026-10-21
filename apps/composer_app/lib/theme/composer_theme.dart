import 'package:flutter/material.dart';

import 'tokens.dart' as t;

/// The app theme: dark, one accent, corners of 2–5px, Inter for UI and JetBrains Mono for ids and URLs.
ThemeData composerTheme() {
  const scheme = ColorScheme.dark(
    primary: t.accent,
    onPrimary: t.bg,
    secondary: t.info,
    onSecondary: t.bg,
    error: t.err,
    onError: t.bg,
    surface: t.surface,
    onSurface: t.text,
    onSurfaceVariant: t.muted,
    outline: t.borderStrong,
    outlineVariant: t.border,
  );
  final shape = RoundedRectangleBorder(borderRadius: BorderRadius.circular(t.radius));
  final base = ThemeData(useMaterial3: true, colorScheme: scheme, fontFamily: t.fontUi);
  return base.copyWith(
    scaffoldBackgroundColor: t.bg,
    canvasColor: t.bg,
    dividerColor: t.border,
    splashFactory: NoSplash.splashFactory,
    textTheme: base.textTheme.apply(bodyColor: t.text, displayColor: t.text),
    dividerTheme: const DividerThemeData(color: t.border, thickness: 1, space: 1),
    appBarTheme: const AppBarTheme(backgroundColor: t.surface, foregroundColor: t.text, elevation: 0, scrolledUnderElevation: 0),
    cardTheme: CardThemeData(color: t.surface, elevation: 0, margin: EdgeInsets.zero, shape: shape.copyWith(side: const BorderSide(color: t.border))),
    filledButtonTheme: FilledButtonThemeData(
      style: FilledButton.styleFrom(backgroundColor: t.accent, foregroundColor: t.bg, shape: shape, textStyle: const TextStyle(fontWeight: FontWeight.w600)),
    ),
    outlinedButtonTheme: OutlinedButtonThemeData(
      style: OutlinedButton.styleFrom(foregroundColor: t.text, side: const BorderSide(color: t.borderStrong), shape: shape),
    ),
    textButtonTheme: TextButtonThemeData(style: TextButton.styleFrom(foregroundColor: t.accent, shape: shape)),
    inputDecorationTheme: InputDecorationTheme(
      filled: true,
      fillColor: t.bg,
      isDense: true,
      hintStyle: const TextStyle(color: t.faint),
      labelStyle: const TextStyle(color: t.muted),
      border: OutlineInputBorder(borderRadius: BorderRadius.circular(t.radius), borderSide: const BorderSide(color: t.borderStrong)),
      enabledBorder: OutlineInputBorder(borderRadius: BorderRadius.circular(t.radius), borderSide: const BorderSide(color: t.borderStrong)),
      focusedBorder: OutlineInputBorder(borderRadius: BorderRadius.circular(t.radius), borderSide: const BorderSide(color: t.accent, width: 2)),
    ),
    listTileTheme: ListTileThemeData(shape: shape, selectedColor: t.text, selectedTileColor: t.raised, iconColor: t.muted),
    bottomSheetTheme: const BottomSheetThemeData(backgroundColor: t.surface, shape: RoundedRectangleBorder(borderRadius: BorderRadius.vertical(top: Radius.circular(t.radius)))),
    tooltipTheme: TooltipThemeData(decoration: BoxDecoration(color: t.overlay, borderRadius: BorderRadius.circular(t.radius), border: Border.all(color: t.borderStrong)), textStyle: const TextStyle(color: t.text, fontSize: 12)),
    dialogTheme: DialogThemeData(
      backgroundColor: t.overlay,
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(t.radius), side: const BorderSide(color: t.borderStrong)),
    ),
    snackBarTheme: SnackBarThemeData(backgroundColor: t.overlay, contentTextStyle: const TextStyle(color: t.text), shape: shape, behavior: SnackBarBehavior.floating),
  );
}

/// Monospace text in a given colour (ids, URLs, code).
TextStyle mono({double size = 12, Color color = t.muted, FontWeight weight = FontWeight.w400}) =>
    TextStyle(fontFamily: t.fontMono, fontSize: size, color: color, fontWeight: weight);
