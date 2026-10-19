#!/usr/bin/env python3
"""Developer-only native Codex contracts; every data root is freshly generated.

Never use this helper as a desktop deletion backend. The program argument selects
an official test executable, not an existing data directory. No live-root option.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import selectors
import signal
import shutil
import sqlite3
import subprocess
import tempfile
import time

VERSION = "0.162.1"
ROOT_ID = "00000000-0000-4000-8000-000000000001"
CHILD_ID = "00000000-0000-4000-8000-000000000002"
GRAND_ID = "00000000-0000-4000-8000-000000000003"
FORK_ID = "00000000-0000-4000-8000-000000000004"
KEEP_ID = "00000000-0000-4000-8000-000000000005"
TIMEOUT = 20
OUTPUT_BUDGET = 4 * 1024 * 1024


class ContractError(RuntimeError):
    pass


class RpcError(ContractError):
    def __init__(self, error):
        self.error = error
        super().__init__(f"RPC {error.get('code')}: {error.get('message')}")


class GeneratedHome:
    """No caller-supplied root and no importing/copying a real Codex store."""

    def __init__(self):
        self._temp = tempfile.TemporaryDirectory(
            prefix="agentbroom-native-contract-", dir="/tmp"
        )
        self.root = Path(self._temp.name).resolve()
        for name in ("home", "codex", "tmp", "config", "cache", "data", "project"):
            (self.root / name).mkdir()
        self.codex = self.root / "codex"
        self.environment = {
            "HOME": str(self.root / "home"),
            "USERPROFILE": str(self.root / "home"),
            "CODEX_HOME": str(self.codex),
            "TMPDIR": str(self.root / "tmp"),
            "TMP": str(self.root / "tmp"),
            "TEMP": str(self.root / "tmp"),
            "XDG_CONFIG_HOME": str(self.root / "config"),
            "XDG_CACHE_HOME": str(self.root / "cache"),
            "XDG_DATA_HOME": str(self.root / "data"),
            "PATH": "/usr/bin:/bin",
            "LANG": "C.UTF-8",
        }
        # Avoid keychain credentials, inherited providers, remote targets and real DB roots.
        config = (
            'cli_auth_credentials_store = "file"\n'
            f"sqlite_home = {json.dumps(str(self.codex))}\n"
            'model_provider = "synthetic_offline"\n'
            'model = "synthetic-model"\n'
            "[model_providers.synthetic_offline]\n"
            'name = "Synthetic offline contract"\n'
            'base_url = "http://127.0.0.1:9/v1"\n'
            'wire_api = "responses"\n'
            "requires_openai_auth = false\n"
        )
        (self.codex / "config.toml").write_text(config)
        self.paths = {}

    def rollout(
        self,
        thread_id,
        *,
        parent=None,
        history=None,
        fork_parent=None,
        archived=False,
        paginated=False,
        physical_id=None,
    ):
        directory = self.codex / (
            "archived_sessions" if archived else "sessions/2026/01/01"
        )
        directory.mkdir(parents=True, exist_ok=True)
        path = (
            directory / f"rollout-2026-01-01T00-00-00-{physical_id or thread_id}.jsonl"
        )
        source = (
            "cli"
            if parent is None
            else {
                "subagent": {"thread_spawn": {"parent_thread_id": parent, "depth": 1}}
            }
        )
        meta = {
            "id": thread_id,
            "session_id": ROOT_ID if parent else thread_id,
            "timestamp": "2026-01-01T00:00:00Z",
            "cwd": str(self.root / "project"),
            "originator": "agentbroom-synthetic-test",
            "cli_version": VERSION,
            "source": source,
            "model_provider": "synthetic_offline",
        }
        if paginated:
            meta["history_mode"] = "paginated"
        if parent:
            meta["parent_thread_id"] = parent
        if fork_parent:
            meta["forked_from_id"] = fork_parent
            meta["forked_from_ordinal_exclusive"] = 1
        if history:
            source_path = self.paths[history]
            meta["forked_from_id"] = history
            meta["history_base"] = {
                "thread_id": history,
                "end_ordinal_exclusive": 1,
                "end_byte_offset": len(
                    source_path.read_bytes().splitlines(keepends=True)[0]
                ),
            }
        records = [
            {
                "timestamp": "2026-01-01T00:00:00Z",
                "type": "session_meta",
                "payload": meta,
            },
            {
                "timestamp": "2026-01-01T00:00:01Z",
                "type": "event_msg",
                "payload": {
                    "type": "user_message",
                    "message": "Synthetic contract only",
                    "kind": "plain",
                },
            },
        ]
        if paginated:
            for ordinal, record in enumerate(records):
                record["ordinal"] = ordinal
        path.write_text("".join(json.dumps(record) + "\n" for record in records))
        self.paths[thread_id] = path
        return path

    def database(self):
        files = list(self.codex.glob("state_*.sqlite"))
        if len(files) != 1:
            raise ContractError("expected exactly one generated native state database")
        return files[0]

    def rows(self):
        # SQLite only ever opens our freshly generated database.
        with sqlite3.connect(self.database()) as db:
            columns = [row[1] for row in db.execute("PRAGMA table_info(threads)")]
            return {
                row[columns.index("id")]: dict(zip(columns, row))
                for row in db.execute("SELECT * FROM threads")
            }

    def spawn_edges(self):
        with sqlite3.connect(self.database()) as db:
            return list(
                db.execute(
                    "SELECT parent_thread_id,child_thread_id,status FROM thread_spawn_edges ORDER BY parent_thread_id,child_thread_id"
                )
            )

    def close(self):
        self._temp.cleanup()


class NativeServer:
    def __init__(self, binary: Path, home: GeneratedHome):
        self.home = home
        self.child = subprocess.Popen(
            [
                str(binary),
                "-c",
                'cli_auth_credentials_store="file"',
                "-c",
                f"sqlite_home={json.dumps(str(home.codex))}",
                "app-server",
                "--listen",
                "stdio://",
            ],
            cwd=home.root / "project",
            env=home.environment,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            start_new_session=True,
        )
        self.selector = selectors.DefaultSelector()
        self.selector.register(self.child.stdout, selectors.EVENT_READ, "stdout")
        self.selector.register(self.child.stderr, selectors.EVENT_READ, "stderr")
        self.buffer = b""
        self.stderr = b""
        self.frames = []
        self.notifications = []
        self.next_id = 0
        self.output_bytes = 0
        try:
            self.request(
                "initialize",
                {
                    "clientInfo": {
                        "name": "agentbroom_native_contract",
                        "version": "1",
                    },
                    "capabilities": {"experimentalApi": True},
                },
            )
            self.send({"method": "initialized"})
        except BaseException:
            self.close()
            raise

    def send(self, frame):
        data = json.dumps(frame).encode() + b"\n"
        self.child.stdin.write(data)
        self.child.stdin.flush()

    def pump(self, deadline):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise ContractError("native RPC timed out")
        ready = self.selector.select(min(remaining, 1))
        if not ready and self.child.poll() is not None:
            raise ContractError("native process exited unexpectedly")
        for key, _ in ready:
            data = os.read(key.fileobj.fileno(), 65536)
            if not data:
                self.selector.unregister(key.fileobj)
                continue
            self.output_bytes += len(data)
            if self.output_bytes > OUTPUT_BUDGET:
                raise ContractError("native output budget exceeded")
            if key.data == "stderr":
                self.stderr += data
            else:
                self.buffer += data
                while b"\n" in self.buffer:
                    line, self.buffer = self.buffer.split(b"\n", 1)
                    if line:
                        self.frames.append(json.loads(line))

    def request(self, method, params):
        self.next_id += 1
        request_id = self.next_id
        self.send({"id": request_id, "method": method, "params": params})
        deadline = time.monotonic() + TIMEOUT
        while True:
            while self.frames:
                frame = self.frames.pop(0)
                if "method" in frame and "id" in frame:
                    raise ContractError("unexpected native server request")
                if frame.get("id") == request_id and "method" not in frame:
                    if "error" in frame:
                        raise RpcError(frame["error"])
                    if "result" not in frame:
                        raise ContractError("native response has no result or error")
                    return frame["result"]
                self.notifications.append(frame)
            self.pump(deadline)

    def deleted(self, count):
        deadline = time.monotonic() + TIMEOUT
        while True:
            self.notifications.extend(self.frames)
            self.frames.clear()
            ids = [
                x["params"]["threadId"]
                for x in self.notifications
                if x.get("method") == "thread/deleted"
            ]
            if len(ids) >= count:
                return ids
            self.pump(deadline)

    def close(self):
        if self.child.stdin and not self.child.stdin.closed:
            try:
                self.child.stdin.close()
            except BrokenPipeError:
                pass
        try:
            self.child.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(self.child.pid, signal.SIGTERM)
            try:
                self.child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(self.child.pid, signal.SIGKILL)
                self.child.wait(timeout=5)
        self.selector.close()
        for pipe in (self.child.stdout, self.child.stderr):
            if pipe:
                pipe.close()


def verify_runtime(binary, home):
    result = subprocess.run(
        [str(binary), "--version"],
        cwd=home.root / "project",
        env=home.environment,
        stdin=subprocess.DEVNULL,
        capture_output=True,
        timeout=TIMEOUT,
    )
    version = result.stdout.decode().strip()
    if result.returncode or not version.startswith("codex-cli "):
        raise ContractError("unrecognized native runtime identity")
    home.runtime_version = version.removeprefix("codex-cli ")


def warm(server, home):
    # Materialize the generated rollout metadata in the official state database.
    for thread_id in home.paths:
        server.request("thread/read", {"threadId": thread_id, "includeTurns": False})
    rows = home.rows()
    if set(rows) != set(home.paths):
        raise ContractError("generated native metadata did not materialize exactly")


def edges(home, pairs):
    with sqlite3.connect(home.database()) as db:
        for parent, child in pairs:
            db.execute(
                "INSERT INTO thread_spawn_edges(parent_thread_id,child_thread_id,status) VALUES(?,?,?) ON CONFLICT(child_thread_id) DO UPDATE SET parent_thread_id=excluded.parent_thread_id,status=excluded.status",
                (parent, child, "closed"),
            )


def native_subtree(home, root):
    seen = {root}
    pending = [root]
    with sqlite3.connect(home.database()) as db:
        while pending:
            parent = pending.pop()
            for (child,) in db.execute(
                "SELECT child_thread_id FROM thread_spawn_edges WHERE parent_thread_id=? AND status IN ('open','closed')",
                (parent,),
            ):
                if child in seen:
                    raise ContractError("synthetic native spawn cycle")
                seen.add(child)
                pending.append(child)
                if len(seen) > 100:
                    raise ContractError("synthetic graph budget")
    return sorted(seen)


def scope_check(agentbroom, home, root, native_ids):
    # The official runtime creates launcher links/sockets outside the rollout corpus.
    # Scan a complete COPY of our generated corpus, not a real-store snapshot or
    # a relaxed policy for live special files. Preserve links so the scanner can reject them.
    shadow = home.root / "observed-rollouts"
    shadow.mkdir()
    for folder in ("sessions", "archived_sessions"):
        source = home.codex / folder
        if source.exists():
            shutil.copytree(source, shadow / folder, symlinks=True)
    scan_environment = {**home.environment, "CODEX_HOME": str(shadow)}
    inspection = subprocess.run(
        [str(agentbroom), "cleanup-scope", root],
        cwd=home.root / "project",
        env=scan_environment,
        stdin=subprocess.DEVNULL,
        capture_output=True,
        timeout=TIMEOUT,
    )
    if inspection.returncode:
        raise ContractError(
            "project scanner rejected generated store: "
            + inspection.stderr.decode()[:400]
        )
    projection = json.loads(inspection.stdout)
    plan = projection["analysis"]
    if (
        plan["sourceKind"] != "codex"
        or plan["canExecute"] is not False
        or plan["executableFiles"]
    ):
        raise ContractError("inspection policy changed")
    evidence = {
        "runtimeVersion": home.runtime_version,
        "rootId": root,
        "observedIds": plan["includedIds"],
        "nativeIds": native_ids,
        "observedComplete": not projection["inventoryPartial"]
        and "graphUnresolved" not in plan["blockedReasons"],
        "nativeComplete": True,
    }
    check = subprocess.run(
        [str(agentbroom), "fixtures", "native-scope"],
        input=json.dumps(evidence).encode(),
        cwd=home.root / "project",
        env=home.environment,
        capture_output=True,
        timeout=TIMEOUT,
        check=True,
    )
    assessment = json.loads(check.stdout)
    if assessment["executionEnabled"] is not False:
        raise ContractError("scope comparison became write permission")
    return assessment


def files(home):
    return {
        str(p.relative_to(home.codex)): hashlib.sha256(p.read_bytes()).hexdigest()
        for folder in ("sessions", "archived_sessions")
        for p in (home.codex / folder).rglob("*.jsonl")
    }


def deleted_verified(server, home, expected, before_rows, before_files, before_edges):
    response = server.request("thread/delete", {"threadId": ROOT_ID})
    if response != {}:
        raise ContractError("unexpected native deletion response")
    ids = server.deleted(len(expected))
    if len(ids) != len(set(ids)) or set(ids) != set(expected):
        raise ContractError("native deletion notification scope mismatch")
    expected_edges = [
        edge
        for edge in before_edges
        if edge[0] not in expected and edge[1] not in expected
    ]
    if home.spawn_edges() != expected_edges:
        raise ContractError("native spawn-edge deletion mismatch")
    after_rows = home.rows()
    if set(after_rows) != set(before_rows) - set(expected):
        raise ContractError("native state-row deletion mismatch")
    for key, row in after_rows.items():
        if row != before_rows[key]:
            raise ContractError("unselected native semantic record changed")
    after_files = files(home)
    for key, old_hash in before_files.items():
        # The logical owner comes from the generated header, never inferred from filename.
        path = home.codex / key
        if key in after_files:
            if after_files[key] != old_hash:
                raise ContractError("unselected rollout changed")
        elif key not in home.expected_removed_files:
            raise ContractError("unexpected rollout removed")
    if set(after_files) != set(before_files) - home.expected_removed_files:
        raise ContractError("native rollout deletion mismatch or unexpected new file")


def run_case(name, binary, agentbroom):
    home = GeneratedHome()
    server = None
    owner = None
    try:
        verify_runtime(binary, home)
        home.rollout(ROOT_ID, paginated=name == "writer")
        home.rollout(KEEP_ID)
        if name in ("subtree", "external-reference", "scope-mismatch"):
            home.rollout(CHILD_ID, parent=ROOT_ID)
            home.rollout(GRAND_ID, parent=CHILD_ID)
        if name == "external-reference":
            home.rollout(FORK_ID, history=ROOT_ID)
        elif name == "subtree":
            home.rollout(FORK_ID, fork_parent=ROOT_ID)
        if name == "segments":
            home.rollout(
                ROOT_ID,
                archived=True,
                physical_id="00000000-0000-4000-8000-000000000006",
            )
        server = NativeServer(binary, home)
        warm(server, home)
        if name in ("subtree", "external-reference"):
            edges(home, [(ROOT_ID, CHILD_ID), (CHILD_ID, GRAND_ID)])
        if name == "scope-mismatch":
            # A generated inconsistent persisted edge graph: headers still describe
            # the child tree, but the native deletion owner no longer reaches it.
            with sqlite3.connect(home.database()) as db:
                db.execute(
                    "DELETE FROM thread_spawn_edges WHERE child_thread_id=?",
                    (CHILD_ID,),
                )
        expected = native_subtree(home, ROOT_ID)
        gate_rows = home.rows()
        gate_files = files(home)
        gate_edges = home.spawn_edges()
        assessment = scope_check(agentbroom, home, ROOT_ID, expected)
        if name == "scope-mismatch":
            if assessment["aligned"] or assessment["missingFromNative"] != [
                CHILD_ID,
                GRAND_ID,
            ]:
                raise ContractError("header/native spawn mismatch was not rejected")
            if (
                home.rows() != gate_rows
                or files(home) != gate_files
                or home.spawn_edges() != gate_edges
            ):
                raise ContractError("scope rejection changed generated records")
            return {
                "case": name,
                "result": "pass",
                "mutation": "none",
                "realDeletionEnabled": False,
            }
        if not assessment["aligned"]:
            raise ContractError(
                "unexpected native scope mismatch: " + json.dumps(assessment)
            )
        before_rows = home.rows()
        before_files = files(home)
        before_edges = home.spawn_edges()
        home.expected_removed_files = set()
        for key in before_files:
            record = json.loads((home.codex / key).read_text().splitlines()[0])
            if record["payload"]["id"] in expected:
                home.expected_removed_files.add(key)
        if name == "external-reference":
            try:
                server.request("thread/delete", {"threadId": ROOT_ID})
            except RpcError as exc:
                if "forked history still references" not in exc.error.get(
                    "message", ""
                ):
                    raise
            else:
                raise ContractError("external fork reference was not protected")
            if (
                files(home) != before_files
                or home.rows() != before_rows
                or home.spawn_edges() != before_edges
            ):
                raise ContractError("rejected deletion modified records")
        elif name == "writer":
            owner = NativeServer(binary, home)
            owner.request("thread/resume", {"threadId": ROOT_ID, "excludeTurns": True})
            before_rows = home.rows()
            before_files = files(home)
            before_edges = home.spawn_edges()
            try:
                server.request("thread/delete", {"threadId": ROOT_ID})
            except RpcError as exc:
                if "active writer" not in exc.error.get("message", ""):
                    raise
            else:
                raise ContractError("external active writer was not protected")
            if (
                files(home) != before_files
                or home.rows() != before_rows
                or home.spawn_edges() != before_edges
            ):
                raise ContractError("writer rejection changed records")
        elif name == "unknown-id":
            try:
                server.request("thread/delete", {"threadId": FORK_ID})
            except RpcError:
                pass
            else:
                raise ContractError("unknown thread reported deletion success")
            if (
                files(home) != before_files
                or home.rows() != before_rows
                or home.spawn_edges() != before_edges
            ):
                raise ContractError("unknown deletion changed records")
        else:
            deleted_verified(
                server, home, expected, before_rows, before_files, before_edges
            )
        return {
            "case": name,
            "result": "pass",
            "nativeScopeCount": len(expected),
            "realDeletionEnabled": False,
        }
    finally:
        if owner:
            owner.close()
        if server:
            server.close()
        home.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--codex-bin", type=Path, default=os.environ.get("AGENTBROOM_TEST_CODEX_BIN")
    )
    parser.add_argument(
        "--agentbroom-bin",
        type=Path,
        default=Path(__file__).resolve().parents[1] / "target/debug/agentbroom",
    )
    parser.add_argument(
        "--case",
        choices=[
            "single",
            "subtree",
            "segments",
            "external-reference",
            "scope-mismatch",
            "unknown-id",
            "writer",
        ],
    )
    args = parser.parse_args()
    if not args.codex_bin:
        parser.error(
            "provide an official test executable; no global CLI fallback"
        )
    cases = (
        [args.case]
        if args.case
        else [
            "single",
            "subtree",
            "segments",
            "external-reference",
            "scope-mismatch",
            "unknown-id",
            "writer",
        ]
    )
    for name in cases:
        print(
            json.dumps(
                run_case(name, args.codex_bin.resolve(), args.agentbroom_bin.resolve())
            ),
            flush=True,
        )


if __name__ == "__main__":
    main()
