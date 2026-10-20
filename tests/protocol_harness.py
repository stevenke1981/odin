"""Small blocking subprocess harness for acceptance scripts, not product code."""
from __future__ import annotations
import json, queue, subprocess, threading, time
from typing import Any

class Peer:
    def __init__(self, command: list[str], framing: str = 'newline', cwd=None, env=None):
        self.framing = framing
        self.process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=subprocess.PIPE, cwd=cwd, env=env)
        self.messages: queue.Queue = queue.Queue()
        self.events: list[dict] = []
        self.errors: list[str] = []
        self.sequence = 0
        threading.Thread(target=self._read, daemon=True).start()
        threading.Thread(target=self._stderr, daemon=True).start()

    def _stderr(self):
        assert self.process.stderr
        for raw in iter(self.process.stderr.readline, b''):
            self.errors.append(raw.decode('utf-8', errors='replace').rstrip())
            self.errors = self.errors[-100:]

    def _read(self):
        assert self.process.stdout
        stream = self.process.stdout
        try:
            while True:
                if self.framing == 'newline':
                    raw = stream.readline()
                    if not raw: break
                else:
                    length = None
                    while True:
                        line = stream.readline()
                        if not line:
                            self.messages.put(EOFError('Peer exited')); return
                        if line in (b'\n', b'\r\n'): break
                        key, value = line.decode().split(':', 1)
                        if key.lower() == 'content-length': length = int(value)
                    if length is None or not 0 <= length <= 8 * 1024 * 1024:
                        raise ValueError('Invalid fixture frame length')
                    raw = stream.read(length)
                    if len(raw) != length: raise EOFError('Truncated peer frame')
                self.messages.put(json.loads(raw))
            self.messages.put(EOFError('Peer exited'))
        except Exception as exc:
            self.messages.put(exc)

    def send(self, value: dict):
        assert self.process.stdin
        raw = json.dumps(value, ensure_ascii=False, separators=(',', ':')).encode('utf-8')
        if self.framing == 'newline': raw += b'\n'
        else: raw = f'Content-Length: {len(raw)}\r\n\r\n'.encode() + raw
        self.process.stdin.write(raw); self.process.stdin.flush()

    def receive(self, request_id: int, timeout: float = 10, dap=False) -> dict:
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            try: message = self.messages.get(timeout=max(0.01, end-time.monotonic()))
            except queue.Empty: break
            if isinstance(message, Exception):
                raise RuntimeError(f'{message}; stderr={self.errors}') from message
            if message.get('request_seq' if dap else 'id') == request_id:
                return message
            self.events.append(message)
        raise TimeoutError(f'Request {request_id} timed out; stderr={self.errors}')

    def request(self, method: str, params: Any = None, timeout: float = 10) -> dict:
        self.sequence += 1
        self.send({'jsonrpc': '2.0', 'id': self.sequence, 'method': method, 'params': params or {}})
        return self.receive(self.sequence, timeout)

    def notify(self, method: str, params: Any = None):
        self.send({'jsonrpc': '2.0', 'method': method, 'params': params or {}})

    def close(self):
        if self.process.stdin:
            try: self.process.stdin.close()
            except (OSError, BrokenPipeError): pass
        try: self.process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            try: self.process.wait(timeout=2)
            except subprocess.TimeoutExpired: self.process.kill(); self.process.wait(timeout=2)
        for stream in [self.process.stdout, self.process.stderr]:
            if stream: stream.close()

    def __enter__(self): return self
    def __exit__(self, *args): self.close()
