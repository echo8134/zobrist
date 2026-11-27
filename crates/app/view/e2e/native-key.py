"""Send a player's keystroke on the end-to-end runner's virtual display."""

import ctypes
import sys


keys = {
    "enter": (b"Return",),
    "tab": (b"Tab",),
    "copy-selection": (b"Control_L", b"c"),
    "paste": (b"Control_L", b"v"),
}[sys.argv[1]]

# DOM key events cannot drive native focus navigation or grant clipboard access.
display_library = ctypes.CDLL("libX11.so.6")
input_library = ctypes.CDLL("libXtst.so.6")
display_library.XOpenDisplay.restype = ctypes.c_void_p
display = ctypes.c_void_p(display_library.XOpenDisplay(None))
if not display:
    sys.exit("Could not open the virtual display")

codes = [
    display_library.XKeysymToKeycode(display, display_library.XStringToKeysym(key))
    for key in keys
]
for code in codes:
    input_library.XTestFakeKeyEvent(display, code, True, 0)
for code in reversed(codes):
    input_library.XTestFakeKeyEvent(display, code, False, 0)
display_library.XCloseDisplay(display)
