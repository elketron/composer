composer app for Linux (x86_64)

Needs, from your distribution:
  - WPE WebKit 2.48 or newer with WPEPlatform (Arch: wpewebkit), for the micro apps
  - GTK 3, libsecret, libepoxy
  - a notification daemon (mako, swaync, dunst, ...) for notifications
  - optional: a Secret Service (gnome-keyring, KWallet with its Secret Service API) to keep
    device tokens in the keyring; without one they go to a file only you can read

Install (per user):
  ./install.sh            # copies to ~/.local/lib/composer-app, links ~/.local/bin/composer-app,
                          # adds the desktop entry
or run ./composer-app from this directory.

If micro apps stay blank, try LIBGL_ALWAYS_SOFTWARE=1 composer-app.
