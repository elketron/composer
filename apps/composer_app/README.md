# composer app

The phone client of composer servers. It builds for Android only (there is no iOS, Linux,
Windows or macOS target); the desktop is `apps/desktop`. Conventions and testing: the app section
of [CLAUDE.md](../../CLAUDE.md).

    flutter run -d <device>      # or: flutter build apk
    flutter analyze && flutter test

End to end on a device or emulator, against a test server (the script makes a pairing code for each
test; the header of `scripts/phone-e2e.sh` says what the test home needs):

    COMPOSER_HOME=<test home> scripts/phone-e2e.sh <device> http://127.0.0.1:7531

A headless emulator: `emulator -avd <name> -read-only -no-window -no-audio`.

Add a server by running `composer pair --name <device>` on it, then scanning the QR code or
pasting its link or `composer login` line into "Add a server". Servers on the same host
are grouped in the switcher.
