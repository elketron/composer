composer desktop (Windows)

The GPUI desktop talks to a composer server; it does not run one. The server runs on Linux,
for example in WSL2. Start the server before opening composer_desktop.exe.

For a server in WSL, set COMPOSER_HOME to its home directory before starting the app. For example,
in PowerShell (replace Ubuntu and yourname with your WSL distribution and Linux user):

  $env:COMPOSER_HOME = '\\wsl.localhost\Ubuntu\home\yourname\.composer'
  .\composer_desktop.exe

WSL must be running, and the server must listen on an address reachable from Windows. The app
reads the workspace's local token from that home. Keep all files in this folder together: the
Chromium runtime and its sandbox bootstrap are included. This Windows build is not code-signed.
