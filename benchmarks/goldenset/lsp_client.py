"""Minimal LSP-over-stdio JSON-RPC client, generic enough for
rust-analyzer and pyright-langserver.

Only implements what `lsp_to_draft.py` needs: initialize handshake,
didOpen, workspace/symbol, textDocument/references,
textDocument/documentSymbol. Not a general-purpose LSP library.
"""

from __future__ import annotations

import json
import subprocess
import threading
import time
from pathlib import Path
from typing import Any


class LspTimeout(RuntimeError):
    pass


class LspClient:
    def __init__(self, cmd: list[str], cwd: Path):
        self.proc = subprocess.Popen(
            cmd,
            cwd=str(cwd),
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        )
        self._id = 0
        self._lock = threading.Lock()
        self._pending: dict[int, dict] = {}
        self._cv = threading.Condition()
        self._open_docs: set[str] = set()
        self._reader = threading.Thread(target=self._read_loop, daemon=True)
        self._reader.start()

    # --- wire protocol -----------------------------------------------

    def _write(self, obj: dict) -> None:
        body = json.dumps(obj).encode("utf-8")
        header = f"Content-Length: {len(body)}\r\n\r\n".encode("ascii")
        self.proc.stdin.write(header + body)
        self.proc.stdin.flush()

    def _read_loop(self) -> None:
        stdout = self.proc.stdout
        while True:
            header = b""
            while not header.endswith(b"\r\n\r\n"):
                chunk = stdout.read(1)
                if not chunk:
                    return
                header += chunk
            length = 0
            for line in header.split(b"\r\n"):
                if line.lower().startswith(b"content-length:"):
                    length = int(line.split(b":", 1)[1].strip())
            body = stdout.read(length)
            try:
                msg = json.loads(body)
            except json.JSONDecodeError:
                continue

            if "method" in msg and "id" in msg:
                # Server->client request. We don't implement any of these
                # (workspace/configuration, window/workDoneProgress/create,
                # etc.) for real, but must respond or some servers stall.
                self._write({"jsonrpc": "2.0", "id": msg["id"], "result": None})
                continue
            if "method" in msg:
                # Notification (diagnostics, $/progress, logs) — ignored.
                continue
            if "id" in msg:
                with self._cv:
                    self._pending[msg["id"]] = msg
                    self._cv.notify_all()

    def request(self, method: str, params: dict, timeout: float = 60.0) -> Any:
        with self._lock:
            self._id += 1
            req_id = self._id
        self._write({"jsonrpc": "2.0", "id": req_id, "method": method, "params": params})

        deadline = time.monotonic() + timeout
        with self._cv:
            while req_id not in self._pending:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise LspTimeout(f"{method} timed out after {timeout}s")
                self._cv.wait(timeout=remaining)
            msg = self._pending.pop(req_id)
        if "error" in msg:
            raise RuntimeError(f"{method} returned LSP error: {msg['error']}")
        return msg.get("result")

    def notify(self, method: str, params: dict) -> None:
        self._write({"jsonrpc": "2.0", "method": method, "params": params})

    # --- convenience ---------------------------------------------------

    def initialize(self, root_path: Path, init_options: dict | None = None) -> Any:
        params = {
            "processId": None,
            "rootUri": root_path.resolve().as_uri(),
            "capabilities": {
                "workspace": {"symbol": {"dynamicRegistration": False}},
                "textDocument": {
                    "references": {"dynamicRegistration": False},
                    "documentSymbol": {
                        "dynamicRegistration": False,
                        "hierarchicalDocumentSymbolSupport": True,
                    },
                },
            },
        }
        if init_options is not None:
            params["initializationOptions"] = init_options
        result = self.request("initialize", params, timeout=120)
        self.notify("initialized", {})
        return result

    def did_open(self, abs_path: Path, language_id: str) -> None:
        uri = abs_path.resolve().as_uri()
        if uri in self._open_docs:
            return
        text = abs_path.read_text(encoding="utf-8", errors="replace")
        self.notify(
            "textDocument/didOpen",
            {
                "textDocument": {
                    "uri": uri,
                    "languageId": language_id,
                    "version": 1,
                    "text": text,
                }
            },
        )
        self._open_docs.add(uri)

    def workspace_symbol(self, query: str, timeout: float = 60.0) -> list[dict]:
        return self.request("workspace/symbol", {"query": query}, timeout=timeout) or []

    def references(self, uri: str, line0: int, char0: int, timeout: float = 60.0) -> list[dict]:
        return self.request(
            "textDocument/references",
            {
                "textDocument": {"uri": uri},
                "position": {"line": line0, "character": char0},
                "context": {"includeDeclaration": False},
            },
            timeout=timeout,
        ) or []

    def document_symbol(self, uri: str, timeout: float = 30.0) -> list[dict]:
        return self.request(
            "textDocument/documentSymbol", {"textDocument": {"uri": uri}}, timeout=timeout
        ) or []

    def shutdown(self) -> None:
        try:
            self.request("shutdown", {}, timeout=10)
            self.notify("exit", {})
        except Exception:
            pass
        try:
            self.proc.stdin.close()
        except Exception:
            pass
        try:
            self.proc.wait(timeout=5)
        except Exception:
            self.proc.kill()
