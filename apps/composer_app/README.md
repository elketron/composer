# composer app

The desktop and mobile client of composer servers (Linux, Android, Windows; macOS where it builds).
Plan: [docs/plan-app.md](../../docs/plan-app.md). Conventions and testing: the app section of
[CLAUDE.md](../../CLAUDE.md).

    flutter run -d linux         # or: flutter build apk
    flutter analyze && flutter test

Add a server by running `composer pair --name <device>` on it, then scanning the QR code (Android)
or pasting its link or `composer login` line into "Add server".
