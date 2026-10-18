# composer patches to flutter_inappwebview_linux 0.1.0-beta.1

Vendored from pub.dev (`flutter_inappwebview_linux-0.1.0-beta.1`), used through
`dependency_overrides`. Every change is marked `composer patch` in the source.

1. **Build against WPE WebKit 2.54+** (`linux/in_app_webview/in_app_webview_settings.cc`):
   `WPE_SETTING_DISABLE_ANIMATIONS` was renamed `WPE_SETTING_REDUCED_MOTION`; use whichever exists.
2. **Blank view with GPU rendering** (`linux/in_app_webview/inappwebview_egl_texture.cc`,
   `in_app_webview.{h,cc}`): the plugin creates each frame's EGLImage on WPE's own (headless)
   EGL display and binds it in Flutter's GL context, which belongs to another EGL display.
   Mesa (AMD, Hyprland/Wayland and headless Weston) shows nothing. The texture now imports the
   frame's DMA-BUF (`GetCurrentDmaBuf`) as an EGLImage on Flutter's current display in its
   populate callback, still zero-copy; the old path remains the fallback.

Drop this copy once upstream ships both fixes.
