#!/usr/bin/env python3
"""Kill one desktop app and wait for all its WebKit processes before replying."""

import ctypes
import os
import select
import signal
import socket
import subprocess
import sys


# Linux reparents orphaned descendants to the nearest living child subreaper.
PR_SET_CHILD_SUBREAPER = 36
if ctypes.CDLL(None, use_errno=True).prctl(PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) < 0:
    error = ctypes.get_errno()
    raise OSError(error, os.strerror(error))

listener = socket.socket(socket.AF_UNIX)
listener.bind("\0" + sys.argv[1])
listener.listen(1)

# The driver also asks the launcher to stop if WebDriver startup fails.
stopping, notify = socket.socketpair()
signal.signal(signal.SIGTERM, lambda *_: notify.send(b"T"))
app = subprocess.Popen(sys.argv[2:])
try:
    app_exit = os.pidfd_open(app.pid)
    ready, _, _ = select.select([listener, stopping, app_exit], [], [])
    client = listener.accept()[0] if listener in ready else None
finally:
    app.kill()
    app.wait()
    while True:
        try:
            os.wait()
        except ChildProcessError:
            break

if client is not None:
    client.sendall(b"stopped\n")
