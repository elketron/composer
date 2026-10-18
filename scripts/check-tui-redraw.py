#!/usr/bin/env python3
"""Checks that the real TUI repaints while typing in Insert mode.

Headless snapshots (`composer script`) always force a draw, so they can't catch a
missing repaint. This runs the TUI in a pseudo-terminal against a running server and
checks that every typed key produces terminal output.

    scripts/check-tui-redraw.py target/debug/composer <COMPOSER_HOME with a running server>
"""
import fcntl, os, pty, re, select, struct, sys, termios, time

binary, home = sys.argv[1], sys.argv[2]
pid, fd = pty.fork()
if pid == 0:
    os.environ.update({"COMPOSER_HOME": home, "TERM": "xterm-256color"})
    os.execv(binary, [binary])
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 36, 120, 0, 0))


def read_for(seconds):
    out, end = b"", time.time() + seconds
    while time.time() < end:
        ready, _, _ = select.select([fd], [], [], 0.05)
        if ready:
            try:
                out += os.read(fd, 65536)
            except OSError:
                break
    return out


try:
    read_for(3)  # start, connect, attach
    os.write(fd, b"i")
    read_for(0.7)
    os.write(fd, b"zqx")
    painted = re.sub(rb"\x1b\[[0-9;?]*[A-Za-z]", b"", read_for(0.7))
    ok = all(bytes([c]) in painted for c in b"zqx")
finally:
    os.kill(pid, 9)
print("ok: typed text is painted while typing" if ok else "FAIL: typing in Insert mode does not repaint")
sys.exit(0 if ok else 1)
