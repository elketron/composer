composer desktop (Linux)

The chat on the left, the stage on the right: pages, terminals, the live view.
It talks to a composer server: the one in your home (~/.composer, or $COMPOSER_HOME), or the
one `composer login` paired this machine with. Start the server first (`composer serve`).

  ./install.sh       install for this user (~/.local)
  composer-desktop   run

Needs: a Wayland or X11 desktop with Vulkan or OpenGL drivers (fonts are built in). The
Chromium runtime (CEF) is in this folder: keep the files together.
Keys: ctrl+1 chat, ctrl+2 stage, ctrl+tab tabs, ctrl+shift+p palette, ctrl+shift+s sessions,
ctrl+shift+h home. More: docs/desktop.md.
