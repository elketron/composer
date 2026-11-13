# composer on a Windows PC (server in WSL)

The server is Linux-only, so on Windows it runs inside WSL2; the app runs natively on Windows.
Everything below is untested on a real work PC so far: correct it as you go.

## 1. WSL and the tools

In PowerShell: `wsl --install -d Ubuntu` (or another distribution), then inside WSL:

```sh
sudo apt install git just unzip            # just: or `cargo install just`
curl -fsSL https://bun.sh/install | bash    # micro apps are built with bun
```

Keep projects inside the WSL filesystem (`~/src/...`), not under `/mnt/c`: git (the changes view)
and file watching are slow across the boundary.

## 2. The server

Take `composer-<version>-linux-x86_64.tar.gz` from a GitHub release (built on Ubuntu 22.04, so it
runs on most distributions), or `cargo install --path crates/cli` from a checkout.

```sh
tar xzf composer-*-linux-x86_64.tar.gz && install composer-*/composer ~/.local/bin/
composer init                 # ~/.composer: seed sites, templates, config.toml
$EDITOR ~/.composer/config.toml   # [provider] base_url / model of your model server
```

Keep it running with systemd (WSL supports it: `/etc/wsl.conf` → `[boot]` `systemd=true`, then
`wsl --shutdown` from PowerShell and reopen):

```ini
# ~/.config/systemd/user/composer.service
[Unit]
Description=composer server

[Service]
ExecStart=%h/.local/bin/composer serve
Restart=on-failure

[Install]
WantedBy=default.target
```

`systemctl --user enable --now composer`. WSL stops when its last window closes unless something
keeps it up; `loginctl enable-linger $USER` keeps user services running.

For the TUI's "open app in browser", let WSL reach the Windows browser: `sudo apt install wslu`
and `export BROWSER=wslview`.

## 3. Reaching it: the app on this PC

WSL2 forwards `localhost`, so the Windows desktop can reach the server at `http://127.0.0.1:7420`.
Point the desktop's `COMPOSER_HOME` at the WSL server's home as described below; it reads the
workspace's token from there.

## 4. Reaching it: your other devices (Tailscale)

Expose it to your tailnet only, with one of:

- **Tailscale on Windows** (simplest if it is already installed): in PowerShell,
  `tailscale serve --bg http://127.0.0.1:7420`. `composer pair` in WSL can't see that Tailscale, so its
  link says `http://127.0.0.1:7420`: on the phone, add the server by hand with
  `https://<pc>.<tailnet>.ts.net` and the code. (Or let WSL use the Windows CLI:
  `ln -s "/mnt/c/Program Files/Tailscale/tailscale.exe" ~/.local/bin/tailscale`; then `composer pair`
  prints the right link and QR code.)
- **Tailscale inside WSL** (its own tailnet node): install it in WSL, `sudo tailscale up`, then
  `tailscale serve --bg http://127.0.0.1:7420`. `composer pair` then shows the tailnet link and QR.

The phone can pair separately with the server using the code from `composer pair`.

## 5. The Windows app

Unpack `composer-desktop-<version>-windows-x86_64.zip` from a release. In PowerShell, point the
desktop at the WSL home (substitute your distribution and Linux username) and run it:

```powershell
$env:COMPOSER_HOME = '\\wsl.localhost\Ubuntu\home\yourname\.composer'
.\composer_desktop.exe
```

Keep the archive's files together. The native GPUI desktop uses the included CEF runtime; the
server and its terminals continue to run in WSL. This setup has not yet been tested on a real PC.
