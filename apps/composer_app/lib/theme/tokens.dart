// The composer design tokens: the same values as templates/sdk/theme.css and the TUI.

import 'package:flutter/painting.dart';

// Surfaces: one step apart, never more than four.
const bg = Color(0xFF0C0D0F);
const surface = Color(0xFF121317);
const raised = Color(0xFF191B20);
const overlay = Color(0xFF1F2228);
/// Your own messages: bg tinted toward the accent.
const you = Color(0xFF151A22);

// Lines.
const border = Color(0xFF24272D);
const borderStrong = Color(0xFF363A42);

// Text.
const text = Color(0xFFD5D8DE);
const muted = Color(0xFF868C96);
const faint = Color(0xFF50555E);

// One accent; links, focus, selection.
const accent = Color(0xFF8FB3E8);

// State.
const ok = Color(0xFF8CC29A);
const warn = Color(0xFFD6B06A);
const err = Color(0xFFE07C7C);
/// The agent: tool rows, thinking.
const info = Color(0xFFB39CE0);

// Corners: 2px for small things (badges, dots, markers), 5px for everything else.
const radiusSm = 2.0;
const radius = 5.0;

const fontUi = 'Inter';
const fontMono = 'JetBrainsMono';
