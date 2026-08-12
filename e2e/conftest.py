"""Shared fixtures for the MCP-driven e2e suite.

The suite drives the packed component through `act run --mcp` over stdio with
a real MCP client, so what the tests observe is what an agent observes.
"""

import base64
import json
import os
import shlex
import subprocess
import pytest
from pathlib import Path

from fastmcp import Client
from fastmcp.client.transports import StdioTransport

# Measured in docs/specs/2026-08-08-e2e-harness-findings.md, question 1.
from mcp.shared.exceptions import McpError

WASM = "target/wasm32-wasip2/release/component_anydoc.wasm"
FIXTURES = Path(__file__).parent / "fixtures"

# ACT's audit trail writes to stderr unconditionally — it is not governed by
# RUST_LOG — so it is redirected to a file rather than left to flood pytest.
LOG_FILE = Path(".pytest-act-stderr.log")


@pytest.fixture(scope="session")
def act_command() -> list[str]:
    """The ACT invocation, honouring the same override the justfile uses.

    Parsed with shlex, not treated as a single path: the justfile's own
    default for its `act` variable is `npx @actcore/act` — two words — which
    cannot be `argv[0]` for a non-shell `subprocess.run`/`StdioTransport`
    call. A bare `os.environ.get("ACT", "act")` string breaks that default;
    splitting it is what makes both forms ("act" on PATH, and the npx
    two-word default) actually spawn.
    """
    return shlex.split(os.environ.get("ACT", "act"))


@pytest.fixture(scope="session")
def wasm_path(act_command: list[str]) -> Path:
    """The packed component.

    Existence is not enough and neither is a fresh mtime: `cargo build`
    produces a wasm with no `act:component` custom section, and an unpacked
    artifact declares no capability ceiling, so every grant is refused as
    "outside ceiling" and the failures point anywhere but here. This has
    already bitten repeatedly in this workspace, so the fixture checks the
    section rather than the file.
    """
    path = Path(WASM)
    if not path.exists():
        pytest.fail(f"{path} is missing — run `just build` first")
    probe = subprocess.run(
        [*act_command, "inspect", "component-manifest", str(path)],
        capture_output=True, text=True,
    )
    name = json.loads(probe.stdout or "{}").get("std", {}).get("name", "unknown")
    if name in ("", "unknown"):
        pytest.fail(f"{path} is built but not packed — run `just pack`")
    return path


@pytest.fixture
async def client(act_command: list[str], wasm_path: Path):
    """An ungranted MCP client, one `act` process per test.

    anydoc's declared ceiling is read-only `wasi:filesystem` for a `path`
    source; a `data` source needs no grant at all. This fixture grants
    nothing, so it proves the ceiling actually denies a `path` read, and it
    is also the host every non-`path` test uses — see `granted_client` for
    the one thing this fixture cannot exercise.
    """
    transport = StdioTransport(
        command=act_command[0],
        args=[*act_command[1:], "run", str(wasm_path), "--mcp"],
        keep_alive=False,  # read-only parser, no state to leak between tests
        log_file=LOG_FILE,
    )
    async with Client(transport) as connected:
        yield connected


@pytest.fixture
async def granted_client(act_command: list[str], wasm_path: Path):
    """An MCP client granted read-only access to e2e/fixtures/.

    The only way to exercise a `path` source actually succeeding: a denied
    call never reaches format detection at all, so `client` cannot stand in
    for this. Fixtures here are static documents checked into the repo, not
    per-test temp files, so the grant targets `e2e/fixtures/` itself — same
    shape as the old justfile's granted host (`--grant
    '{"wasi:filesystem":{"mode":"allowlist","allow":[{"path":"<fixtures_dir>/**","mode":"ro"}]}}'`).
    """
    grant = json.dumps({
        "wasi:filesystem": {
            "mode": "allowlist",
            "allow": [{"path": f"{FIXTURES}/**", "mode": "ro"}],
        }
    })
    transport = StdioTransport(
        command=act_command[0],
        args=[*act_command[1:], "run", str(wasm_path), "--mcp", "--grant", grant],
        keep_alive=False,
        log_file=LOG_FILE,
    )
    async with Client(transport) as connected:
        yield connected


@pytest.fixture
def fixtures_dir() -> Path:
    """Absolute path to `e2e/fixtures/`, for the one test that needs a real
    on-disk path rather than inline `data` bytes."""
    return FIXTURES


@pytest.fixture
def fixture_bytes():
    """Load a document under `e2e/fixtures/` as the transport's canonical
    `{"$bytes": "<base64>"}` byte-string envelope for a `data` argument.

    The old `e2e/fixtures/args/*.json` files were pre-assembled ACT-HTTP
    request bodies wrapping these same fixture bytes (`generate.py` builds
    both from one source). Reading the binary fixtures directly here keeps
    that single source of truth without carrying the now-unused
    HTTP-envelope layer forward.
    """

    def _load(name: str) -> dict:
        raw = (FIXTURES / name).read_bytes()
        return {"$bytes": base64.b64encode(raw).decode()}

    return _load


@pytest.fixture
def expect_error():
    """Assert a call fails with a specific ACT error kind.

    Exposed as a fixture rather than a plain function so tests never have to
    import from `conftest` — that import only resolves when the test
    directory happens to be on `sys.path`, which is not something to rely on.

    Measured, not assumed. `call-tool` in `act:tools` returns a bare
    `tool-result` with NO `result<>` wrapper — only `list-tools` has one — so
    a guest reporting a failed tool call can only do it through
    `tool-event::error`, which arrives as a result with `is_error` set and the
    kind in `_meta`. **That is the path a tool test will take.**

    The JSON-RPC error path exists for failures that are not the guest's tool
    body: `list-tools`, the session operations, a wasmtime trap, an
    unreachable actor. It raises `mcp.shared.exceptions.McpError` with the
    payload at `exc.error.data`. No tool test in this suite is expected to
    reach it, but both are handled here so callers need not care.
    """

    async def _expect(client, tool: str, arguments: dict, kind: str):
        try:
            result = await client.call_tool(tool, arguments, raise_on_error=False)
        except McpError as exc:
            data = getattr(getattr(exc, "error", None), "data", None) or {}
            assert data.get("dev.actcore/error-kind") == kind, (
                f"expected {kind} on the JSON-RPC error path, got {data!r}"
            )
            return

        assert result.is_error, f"expected {tool} to fail, got {result!r}"
        meta = result.meta or {}
        assert meta.get("dev.actcore/error-kind") == kind, (
            f"expected {kind} on the isError path, got {meta!r}"
        )

    return _expect
