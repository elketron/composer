# composer app

The phone client of composer servers (Android; iOS later). The desktop is `apps/desktop`.
Plan: [docs/plan-app.md](../../docs/plan-app.md). Conventions and testing: the app section of
[CLAUDE.md](../../CLAUDE.md).

    flutter run -d <device>      # or: flutter build apk
    flutter analyze && flutter test

End to end on a device or emulator, against a rehearsal server (it sets up the pairing, the scripted
model and the `tools` site):

    COMPOSER_HOME=<test home> scripts/phone-e2e.sh <device> http://127.0.0.1:7531/w/default

A headless emulator: `emulator -avd <name> -read-only -no-window -no-audio`.

Add a server by running `composer pair --name <device>` on it, then scanning the QR code or pasting
its link or `composer login` line into "Add server".
