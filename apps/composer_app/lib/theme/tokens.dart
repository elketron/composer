// The phone's design tokens: the roles below, filled with the Noema palette the desktop uses
// (docs/mockups/next/tokens.css). Only the colours follow Noema; sizes, type and corners are the phone's.

import 'package:flutter/painting.dart';

// Surfaces: one step apart, never more than four.
const bg = Color(0xFF0E0E0F);
const surface = Color(0xFF131315);
const raised = Color(0xFF171718);
const overlay = Color(0xFF1A1A1C);
/// Your own messages: bg tinted toward the accent.
const you = Color(0xFF12111A);

// Lines.
const border = Color(0xFF222224);
const borderStrong = Color(0xFF2A2A2C);

// Text.
const text = Color(0xFFC2C0B6);
const muted = Color(0xFF8A8880);
const faint = Color(0xFF6A6865);

// One accent; links, focus, selection.
const accent = Color(0xFF8A82CC);

/// Filled controls (Approve, send, New chat): the deep indigo with bright text, as the desktop's Allow.
const accentFill = Color(0xFF4E46A0);
const onAccent = Color(0xFFD8D6CC);

// State.
const ok = Color(0xFF4CAF82);
const warn = Color(0xFFD4A843);
const err = Color(0xFFE05A5A);
/// The agent: tool rows, thinking.
const info = Color(0xFF8A82CC);

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
