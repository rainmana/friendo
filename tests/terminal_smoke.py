"""Unix PTY user journeys; standard-library QA only, never a product dependency."""
import codecs
import errno
import fcntl
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import os
from pathlib import Path
import pty
import re
import select
import signal
import struct
import subprocess
import tempfile
import termios
import threading
import time
import unicodedata

ROOT = Path(__file__).resolve().parents[1]
BIN = Path(os.environ.get("FRIENDO_BIN", ROOT / "target/debug/friendo")).resolve()
FIXTURE = Path(os.environ.get("FRIENDO_FIXTURE_BIN", BIN.with_name("friendo-fixture"))).resolve()
CAPTURES = ROOT / "docs/captures"


class Screen:
    """Decode the cursor, erase, and styling subset emitted by Crossterm/Ratatui.

    This is a test observer, not the application's deferred process PTY emulator.
    CSI state survives reads so assertions use cells, not concatenated repaint bytes.
    """
    def __init__(self):
        self.cells = {}
        self.row = self.column = 0
        self.pending = ""

    def feed(self, text):
        text = self.pending + text
        self.pending = ""
        while text:
            if text.startswith("\x1b["):
                match = re.match(r"\x1b\[([0-?]*)([ -/]*)([@-~])", text)
                if not match:
                    self.pending = text
                    return
                params, _, operation = match.groups()
                if not params.startswith("?"):
                    values = [int(v or 0) for v in params.split(";")]
                    value = values[0] or 1
                    if operation in "Hf":
                        self.row = value - 1
                        self.column = (values[1] or 1) - 1 if len(values) > 1 else 0
                    elif operation == "G":
                        self.column = value - 1
                    elif operation == "d":
                        self.row = value - 1
                    elif operation in "ABCD":
                        self.row = max(0, self.row + {"A": -value, "B": value}.get(operation, 0))
                        self.column = max(0, self.column + {"D": -value, "C": value}.get(operation, 0))
                    elif operation in "JK":
                        mode = values[0]
                        for row, column in list(self.cells):
                            after = (row, column) >= (self.row, self.column)
                            before = (row, column) <= (self.row, self.column)
                            if (operation == "J" or row == self.row) and (mode in (2, 3) or mode == 0 and after or mode == 1 and before):
                                del self.cells[row, column]
                text = text[match.end():]
            elif text == "\x1b":
                self.pending = text
                return
            else:
                character, text = text[0], text[1:]
                if character == "\r":
                    self.column = 0
                elif character == "\n":
                    self.row += 1
                elif ord(character) >= 32 and character != "\x7f":
                    if unicodedata.combining(character):
                        key = self.row, max(0, self.column - 1)
                        self.cells[key] = self.cells.get(key, "") + character
                    else:
                        self.cells[self.row, self.column] = character
                        self.column += 2 if unicodedata.east_asian_width(character) in ("W", "F") else 1

    def text(self):
        return "\n".join("".join(self.cells.get((row, column), " ") for column in range(300))
                         for row in range(100))


class Terminal:
    def __init__(self, directory, config, command, name):
        self.runs = directory / "runs"
        self.started = time.monotonic()
        self.deadline = self.started + 45
        self.text = ""
        self.screen_state = Screen()
        self.decoder = codecs.getincrementaldecoder("utf-8")("replace")
        self.status = None
        CAPTURES.mkdir(parents=True, exist_ok=True)
        self.capture = CAPTURES / f"{name}.cast"
        self.cast = self.capture.open("w")
        self.cast.write(json.dumps({"version": 2, "width": 140, "height": 44,
                                   "title": f"friendo native Unix QA / {name}",
                                   "env": {"TERM": "xterm-256color"}}) + "\n")
        self.pid, self.master = pty.fork()
        if self.pid == 0:
            os.chdir(directory)
            fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack("HHHH", 44, 140, 0, 0))
            os.environ["TERM"] = "xterm-256color"
            os.environ["COLORTERM"] = "truecolor"
            os.execl(BIN, "friendo", "--config", str(config), "--data-dir", str(self.runs), *command)
        self.initial = termios.tcgetattr(self.master)

    def pump(self):
        if select.select([self.master], [], [], 0.02)[0]:
            try:
                data = os.read(self.master, 65536)
            except OSError as error:
                if error.errno != errno.EIO:
                    raise
                data = b""
            if data:
                text = self.decoder.decode(data)
                self.text += text
                self.screen_state.feed(text)
                self.cast.write(json.dumps([time.monotonic() - self.started, "o", text]) + "\n")
        if self.status is None:
            child, status = os.waitpid(self.pid, os.WNOHANG)
            if child:
                self.status = status

    def wait(self, predicate, description):
        deadline = min(self.deadline, time.monotonic() + 12)
        while time.monotonic() < deadline:
            self.pump()
            if predicate():
                return
            if self.status is not None:
                raise AssertionError(f"TUI exited while waiting for {description}; capture {self.capture}")
        raise AssertionError(f"timed out waiting for {description}; capture {self.capture}")

    def screen(self, marker, since=0):
        self.wait(lambda: len(self.text) > since and marker in self.screen_state.text(), repr(marker))

    def events(self):
        paths = list(self.runs.glob("*/events.jsonl"))
        if not paths:
            return []
        # Only newline-terminated events are authoritative while the child appends.
        return [json.loads(line) for line in paths[0].read_bytes().splitlines(keepends=True)
                if line.endswith(b"\n")]

    def event(self, kind, predicate=lambda item: True):
        self.wait(lambda: any(e["kind"]["type"] == kind and predicate(e["kind"])
                              for e in self.events()), f"journal {kind}")

    def send(self, data):
        os.write(self.master, data)
        self.cast.write(json.dumps([time.monotonic() - self.started, "i", data.decode("utf-8")]) + "\n")

    def compose(self, text):
        start = len(self.text)
        self.send(b"i")
        self.screen("COMPOSER", start)
        self.send(b"\x1b[200~" + text.encode() + b"\x1b[201~\r")

    def resize(self, width, height):
        start = len(self.text)
        fcntl.ioctl(self.master, termios.TIOCSWINSZ, struct.pack("HHHH", height, width, 0, 0))
        os.kill(self.pid, signal.SIGWINCH)
        self.screen("LIST" if width < 90 else "TOPOLOGY", start)

    def finish(self):
        self.send(b"q")
        self.wait(lambda: self.status is not None, "clean quit")
        # Drain cleanup emitted just before waitpid reports the exit.
        while select.select([self.master], [], [], 0)[0]:
            old = len(self.text)
            self.pump()
            if old == len(self.text):
                break
        final = termios.tcgetattr(self.master)
        assert os.waitstatus_to_exitcode(self.status) == 0, self.status
        for control in ["\x1b[?1049l", "\x1b[?25h", "\x1b[?2004l"]:
            assert control in self.text, repr(control)
        mask = termios.ICANON | termios.ECHO | termios.ISIG
        assert final[3] & mask == self.initial[3] & mask, "terminal modes not restored"

    def close(self):
        if self.status is None:
            os.kill(self.pid, signal.SIGTERM)
            os.waitpid(self.pid, 0)
        os.close(self.master)
        self.cast.close()


def demo(directory):
    config = directory / "demo.toml"
    config.write_text("effects = 'full'\n")
    terminal = Terminal(directory, config, ["demo"], "demo")
    try:
        terminal.screen("OFFLINE DEMO")
        terminal.send(b" ")
        terminal.event("node", lambda e: e["node"]["id"] == "answer")
        terminal.send(b" ")
        terminal.event("correction", lambda e: e["revision"] == 2)
        terminal.send(b"r")
        terminal.screen("REWIRE")
        terminal.send(b"\r")
        terminal.event("restored")
        terminal.screen("LINK RESTORED")
        terminal.send(b"y")
        terminal.event("autonomy", lambda e: e["mode"] == "yolo")
        terminal.send(b"1")
        terminal.resize(60, 24)
        terminal.resize(140, 44)
        before = terminal.events()
        start = len(terminal.text)
        terminal.send(b"[y")
        terminal.screen("Autonomy cannot change during review", start)
        assert terminal.events() == before, "historical controls changed the journal"
        terminal.send(b"\x1b")
        terminal.finish()
    finally:
        terminal.close()
    return {"demo": "passed", "resize": "140x44 -> 60x24 -> 140x44", "capture": str(terminal.capture)}


class Provider(HTTPServer):
    def __init__(self):
        super().__init__(("127.0.0.1", 0), Handler)
        self.requests = []
        self.errors = []
        self.held = {count: threading.Event() for count in (3, 4)}
        self.release = {count: threading.Event() for count in (3, 4)}


def delta(value, finish=None):
    return "data: " + json.dumps({"choices": [{"index": 0, "delta": value, "finish_reason": finish}]}) + "\n\n"


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_POST(self):
        try:
            assert self.path == "/v1/chat/completions", self.path
            body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            self.server.requests.append(body)
            count = len(self.server.requests)
            if count == 1:
                args = {"executable": str(FIXTURE), "args": ["args", "PTY_TOOL_OK"]}
                response = delta({"tool_calls": [{"index": 0, "id": "pty-call", "type": "function",
                                                  "function": {"name": "exec", "arguments": json.dumps(args)}}]})
                response += delta({}, "tool_calls") + "data: [DONE]\n\n"
            elif count == 2:
                response = delta({"content": "PTY_FINAL_OK"}) + delta({}, "stop") + "data: [DONE]\n\n"
            elif count in (3, 4):
                # A held real HTTP stream gives correction and Ctrl-C a deterministic target.
                response = delta({"content": f"PTY_HELD_STREAM_{count}"})
            else:
                raise AssertionError("unexpected extra model request")
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Connection", "close")
            self.end_headers()
            for offset in range(0, len(response), 17):
                self.wfile.write(response[offset:offset + 17].encode())
                self.wfile.flush()
            if count in (3, 4):
                self.server.held[count].set()
                assert self.server.release[count].wait(20), "held response was never released"
        except (BrokenPipeError, ConnectionResetError):
            pass  # Cancellation closes the deliberately unfinished third response.
        except Exception as error:
            self.server.errors.append(repr(error))


def live(directory):
    server = Provider()
    config = directory / "live.toml"
    config.write_text(f"""active_provider = 'fixture'
effects = 'off'
color = false
max_turns = 3
[execution]
allowed_executables = [{json.dumps(str(FIXTURE))}]
allow_shell = false
[keys]
repair = 'x'
yolo = 'z'
trace = 't'
pin = 'p'
[providers.fixture]
endpoint = 'http://127.0.0.1:{server.server_port}/v1'
model = 'explicit-terminal-fixture'
""")
    # Fork before starting the fixture's Python thread.
    terminal = Terminal(directory, config, ["run"], "live-fixture")
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    goal = "Inspect harmless café ☃ fixture"
    try:
        terminal.screen("AUTONOMY: NORMAL")
        terminal.compose(goal)
        terminal.event("model_finished", lambda e: e["text"] == "PTY_FINAL_OK")
        events = terminal.events()
        assert len(server.requests) == 2
        results = [e["kind"]["result"] for e in events if e["kind"]["type"] == "tool_finished"]
        assert len(results) == 1 and results[0]["stdout_excerpt"] == "PTY_TOOL_OK\n"
        message = next(m for m in server.requests[1]["messages"] if m["role"] == "tool")
        assert json.loads(message["content"]) == results[0]
        for event, request in zip([e for e in events if e["kind"]["type"] == "model_requested"], server.requests):
            journal = next(terminal.runs.glob("*/events.jsonl"))
            assert json.loads((journal.parent / event["kind"]["context"]).read_text()) == request
        assert any(e["kind"].get("statement") == goal for e in events), "Unicode paste changed the goal"
        # Inspect retained context in the native interface.
        start = len(terminal.text)
        terminal.send(b"4\r")
        terminal.screen("PTY_TOOL_OK", start)
        # Enter closes the inspector. ESC followed immediately by 1 is an Alt-1 chord.
        terminal.send(b"\r1")
        terminal.compose('/constraints ["Preserve original evidence"]')
        terminal.event("constraints", lambda e: e["items"] == ["Preserve original evidence"])
        terminal.send(b"z")
        terminal.event("autonomy", lambda e: e["mode"] == "yolo")
        # Remapped Rewire, then correction while a streamed response is held.
        terminal.send(b"x")
        terminal.screen("REWIRE")
        terminal.send(b"\r")
        terminal.wait(server.held[3].is_set, "held fixture stream for correction")
        terminal.send(b"x")
        terminal.send(b"\x1b[200~ corrected\x1b[201~\r")
        terminal.event("correction", lambda e: e["revision"] == 4)
        terminal.event("failure")
        server.release[3].set()
        terminal.compose("/continue")
        terminal.wait(server.held[4].is_set, "held fixture stream for Ctrl-C")
        terminal.send(b"\x03")
        terminal.event("cancel_requested", lambda e: e["reason"] == "Ctrl-C")
        terminal.finish()
        events = terminal.events()
        assert not any(e["kind"]["type"] == "model_finished" and e["kind"].get("text", "").startswith("PTY_HELD_STREAM") for e in events)
        # Constraints and corrections are distinct events; inspect the application's reducer.
        journal = next(terminal.runs.glob("*/events.jsonl"))
        replay = subprocess.run([BIN, "--config", config, "--data-dir", terminal.runs,
                                 "replay", journal.parent.name, "--headless"],
                                check=True, capture_output=True, timeout=15)
        state = json.loads(replay.stdout)
        assert state["constraints"] == ["Preserve original evidence"]
        assert state["revision"] == 4 and state["autonomy"] == "yolo"
        for request in server.requests[2:]:
            assert json.loads(request["messages"][1]["content"])["explicit_constraints"] == state["constraints"]
        assert len(server.requests) == 4 and not server.errors, server.errors
    finally:
        for release in server.release.values():
            release.set()
        terminal.close()
        server.shutdown()
        server.server_close()
        worker.join(timeout=5)
    return {"live_fixture": "passed", "model_requests": 4, "real_tools": 1,
            "unicode_paste": "passed", "correction_cancel": "passed", "capture": str(terminal.capture)}


def main():
    assert BIN.is_file() and FIXTURE.is_file(), "build bins first, or set FRIENDO_BIN/FRIENDO_FIXTURE_BIN"
    with tempfile.TemporaryDirectory(prefix="friendo-terminal-") as directory:
        base = Path(directory)
        (base / "demo").mkdir()
        (base / "live").mkdir()
        print(json.dumps(demo(base / "demo")))
        print(json.dumps(live(base / "live")))


if __name__ == "__main__":
    main()
