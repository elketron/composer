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

// Corners (phone only; the TUI stays square). Cards and sheets 5, buttons, inputs and chips 4,
// small labels 3, status dots 2. The live view and the bottom bar are square, edge to edge.
const radiusDot = 2.0;
const radiusSm = 3.0;
const radiusControl = 4.0;
const radiusCard = 5.0;

/// Message bubbles, cards and everything not named above.
const radius = radiusCard;

const fontUi = 'Inter';
const fontMono = 'JetBrainsMono';
