# Spike: embedded webview for micro apps (milestone 1 of plan-app.md)

Date: 2026-10-18. Code: `apps/spikes/webview_spike/` (throwaway), patched plugin: `apps/vendor/flutter_inappwebview_linux/`.

## Question

Can the Flutter app show composer's micro apps (the ticket board, the chat app) **inside** its window,
with Flutter able to draw over them, on Linux (Hyprland/Wayland) and Android, and do the apps work
there (cookie sign-in, live WebSocket, `send()`, input)?

## Result: yes, with `flutter_inappwebview` 6.2 (beta) and two patches on Linux

| check | Linux (Hyprland, AMD) | Android (emulator, API 36) |
|---|---|---|
| builds | ✅ after patch 1 (WPE 2.54) | ✅ |
| page loads, scripts run | ✅ | ✅ |
| cookie sign-in, live WebSocket | ✅ connected in 0–250 ms | ✅ connected in 0 ms |
| `send()` reaches the session (and the agent acts on it) | ✅ | not tested (same SDK code) |
| renders inside the Flutter window | ✅ after patch 2 (GPU, zero-copy) | ✅ |
| Flutter draws over the web content | ✅ | ✅ |
| keyboard, mouse, resize, second app (chat) via shared cookie | ✅ (tested by hand) | – |

## Findings

1. **Stable 6.1.5 has no Linux support**; Linux needs the 6.2 beta line (`flutter_inappwebview_linux` 0.1.0-beta.1, WPE WebKit 2.0 platform API, rendered into a Flutter texture).
2. **It doesn't build against WPE WebKit 2.54**: `WPE_SETTING_DISABLE_ANIMATIONS` was renamed `WPE_SETTING_REDUCED_MOTION`. One-line patch.
3. **GPU rendering shows a blank (transparent) view**, both on Hyprland and in headless Weston: the plugin binds an EGLImage created on WPE's EGL display inside Flutter's context (another display). Not Impeller (same with Skia), not the GPU choice (this machine has two AMD GPUs; forcing `WPE_DRM_DEVICE` didn't help). Fixed by importing each frame's DMA-BUF on Flutter's current EGL display in the texture callback. Still zero-copy.
4. Software rendering (`LIBGL_ALWAYS_SOFTWARE=1`) worked before the patch: a fallback if the GPU path breaks somewhere.
5. On Linux, `initialUrlRequest` is unreliable (upstream #2903); the app loads the URL explicitly when the view is created.
6. Testing without a desktop works: headless Weston (`--renderer=gl --debug`, `weston-screenshooter` while the app paints) and a windowless Android emulator (`adb exec-out screencap`).
7. Unrelated: the ticket board's four columns are cramped on a phone; the micro apps need a mobile layout pass.

## Decision

- **All platforms use `flutter_inappwebview` in-window.** Linux uses the vendored, patched `flutter_inappwebview_linux` via `dependency_overrides` until upstream has both fixes (worth reporting / sending upstream).
- Keep a **software-rendering fallback** on Linux (`LIBGL_ALWAYS_SOFTWARE=1` when the GPU path fails), and the system browser as the last resort.
- Re-check before releases: the Linux package is beta.
