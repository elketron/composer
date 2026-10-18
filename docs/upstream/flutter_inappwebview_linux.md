# Draft: report to flutter_inappwebview (not sent)

Target: https://github.com/pichillilorenzo/flutter_inappwebview — as an issue, or as a pull request with
[`flutter_inappwebview_linux.patch`](flutter_inappwebview_linux.patch) (against the pub.dev
`flutter_inappwebview_linux` 0.1.0-beta.1 sources; apply with `patch -p1` from the repository root,
and check it against current `main` first). Could also be split into two issues.

---

**Title:** [Linux] Build fails with WPE WebKit 2.54, and the view stays blank with GPU rendering (EGLImage bound on another EGL display)

**Environment**

- flutter_inappwebview 6.2.0-beta.3, flutter_inappwebview_linux 0.1.0-beta.1
- Flutter 3.47.7 (stable), Impeller and Skia both tried
- WPE WebKit 2.54.1 (WPEPlatform API), Mesa, AMD GPUs (discrete + integrated)
- Hyprland (Wayland); also reproduced in headless Weston (`--renderer=gl`)

**1. Build failure with WPE WebKit 2.54**

```
in_app_webview_settings.cc:405:44: error: use of undeclared identifier 'WPE_SETTING_DISABLE_ANIMATIONS'
```

WPE 2.54 renamed the setting to `WPE_SETTING_REDUCED_MOTION`. Fix: use `WPE_SETTING_DISABLE_ANIMATIONS`
when it is defined, else `WPE_SETTING_REDUCED_MOTION`.

**2. Blank (transparent) webview with hardware rendering**

The page loads and runs (navigation events, JavaScript, WebSockets all work), and the log says
`InAppWebViewEGLTexture: created (zero-copy mode)` / `using GL texture (hardware accelerated)`, but the
view area shows nothing. With `LIBGL_ALWAYS_SOFTWARE=1` the page renders correctly.

Cause, as far as I can tell: `OnWpePlatformBufferRendered` imports each DMA-BUF buffer with
`wpe_buffer_import_to_egl_image()`, which creates the EGLImage on WPE's own (headless) EGL display.
`InAppWebViewEGLTexture` then binds that image with `glEGLImageTargetTexture2DOES` inside Flutter's GL
context, which belongs to a different EGL display. EGLImages aren't guaranteed to be usable across
displays, and on Mesa they aren't. Not related to Impeller (same with Skia) or to the DRM device choice
(`WPE_DRM_DEVICE` pointing at the display GPU doesn't change it).

Fix in the attached patch: the webview exposes the current frame's DMA-BUF planes
(`GetCurrentDmaBuf`: fds, offsets, strides, fourcc, modifier, plus a per-frame serial), and the texture's
populate callback imports them with `eglCreateImage(EGL_LINUX_DMA_BUF_EXT)` on the display that is
current there (Flutter's). Still zero-copy; the existing path stays as the fallback if the import fails.
With it, pages render with hardware acceleration on the setup above, and Flutter widgets draw over the
view as expected.

**Also seen:** `initialUrlRequest` is ignored on Linux (as in #2903); calling `loadUrl` from
`onWebViewCreated` works around it.
