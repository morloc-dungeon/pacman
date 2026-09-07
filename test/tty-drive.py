#!/usr/bin/env python3
"""Drive the game under a pseudo-terminal and print what it drew.

The TUI cannot be exercised by `make test`: it needs a terminal, and a morloc
pool only reaches one by taking the terminal's foreground process group. This
script gives it a pty, feeds it keystrokes, and dumps the screen, which is
enough to catch a broken render, a terminal that is never restored, or a save
that does not round-trip.

    python3 test/tty-drive.py hhhhhhhjjjs     # walk, then save and quit
    python3 test/tty-drive.py q               # start, quit immediately

It is timing-based, so it is a manual check rather than part of `make test`.
"""

import fcntl
import os
import pty
import select
import struct
import sys
import termios
import time

KEY_INTERVAL = 0.4
SETTLE = 2.0


def main(keys, argv):
    pid, fd = pty.fork()
    if pid == 0:
        os.environ["TERM"] = "xterm"
        os.execvp(argv[0], argv)

    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 60, 0, 0))

    out = bytearray()
    start = time.time()
    sent = 0
    while True:
        now = time.time() - start
        if sent < len(keys) and now > 1.0 + sent * KEY_INTERVAL:
            os.write(fd, keys[sent].encode())
            sent += 1
        elif sent >= len(keys) and now > 1.0 + sent * KEY_INTERVAL + SETTLE:
            break
        if now > 30:
            break
        ready, _, _ = select.select([fd], [], [], 0.2)
        if not ready:
            continue
        try:
            chunk = os.read(fd, 65536)
        except OSError:
            break
        if not chunk:
            break
        out += chunk

    for closer in (lambda: os.close(fd), lambda: os.waitpid(pid, 0)):
        try:
            closer()
        except OSError:
            pass
    sys.stdout.write(out.decode("utf-8", "replace"))


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "q", sys.argv[2:] or ["./pacman", "run"])
