#!/usr/bin/env python3
"""Drives the real binary in a pseudo-terminal (Unix only).

Checks that quitting, Ctrl+C, SIGTERM and SIGHUP all leave the alternate screen,
that a too-small terminal shows the resize message, and that $VISUAL hand-off works.
Usage: python3 tests/pty_smoke.py target/debug/jobi
"""
import fcntl, os, pty, select, signal, sqlite3, struct, sys, tempfile, termios, time

BIN = os.path.abspath(sys.argv[1])
TMP = tempfile.mkdtemp(prefix="jobi-smoke-")


def session(cols, rows, steps, env=None, db="smoke.sqlite3"):
    """steps: bytes to type, or a signal number to send. Returns (output, exit status)."""
    pid, fd = pty.fork()
    if pid == 0:
        os.environ.update(env or {})
        os.environ["JOBI_CONFIG"] = os.path.join(TMP, "none.toml")
        os.execv(BIN, ["jobi", "--db", os.path.join(TMP, db)])
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
    out = b""

    def drain(seconds):
        nonlocal out
        end = time.time() + seconds
        while time.time() < end:
            if select.select([fd], [], [], 0.05)[0]:
                try:
                    chunk = os.read(fd, 65536)
                except OSError:
                    return
                out += chunk

    # Wait for the first screen instead of sleeping: a slow machine would otherwise
    # deliver Esc and the next key together, which terminals read as Alt+key.
    deadline = time.time() + 15
    while time.time() < deadline and not ("JOBI" in out.decode("utf8", "replace") or "Resize" in out.decode("utf8", "replace")):
        drain(0.1)
    drain(0.3)
    for step in steps:
        if isinstance(step, int):
            os.kill(pid, step)
        else:
            os.write(fd, step)
        drain(0.6)
    status = None
    for _ in range(60):
        done, st = os.waitpid(pid, os.WNOHANG)
        if done:
            status = st
            break
        drain(0.05)
    if status is None:
        os.kill(pid, signal.SIGKILL)
        os.waitpid(pid, 0)
    return out.decode("utf8", "replace"), status


failures = []


def check(name, cond, detail=""):
    print(("ok   " if cond else "FAIL ") + name)
    if not cond:
        failures.append(f"{name} {detail}")


for name, size, steps in [
    ("quit with q", (120, 35), [b"?", b"x", b"q"]),
    ("quit from narrow layout", (52, 19), [b"a", b"\x1b", b"q"]),
    ("Ctrl+C inside the add form", (100, 30), [b"a", b"Acme", b"\x03"]),
    ("SIGTERM", (100, 30), [signal.SIGTERM]),
    ("SIGHUP", (100, 30), [signal.SIGHUP]),
]:
    out, status = session(*size, steps)
    check(f"{name}: process exits", status is not None and os.WIFEXITED(status) and os.WEXITSTATUS(status) == 0, f"status={status}")
    check(f"{name}: screen restored", out.count("[?1049h") == out.count("[?1049l") >= 1)

out, status = session(30, 10, [b"\x03"])
check("too-small terminal shows resize message", "Resize" in out and status is not None and os.WEXITSTATUS(status) == 0)

# Editor hand-off: add an application, open the posting form, edit the description externally.
keys = [b"a", b"Acme", b"\t", b"Eng", b"\r", b"p", b"https://x", b"\t", b"\x05", b"\r", b"q"]
out, status = session(100, 30, keys, env={"VISUAL": "printf 'from editor' >"}, db="editor.sqlite3")
check("editor hand-off restores the screen twice", out.count("[?1049l") == 2, f"leaves={out.count('[?1049l')}")
row = sqlite3.connect(os.path.join(TMP, "editor.sqlite3")).execute("select description from postings").fetchone()
check("text from the editor was saved", row == ("from editor",), str(row))

if failures:
    print("\n".join(failures))
    sys.exit(1)
