//! Drive the packed anydoc component through `act run --mcp` with a real MCP
//! client.
//!
//! This replaces the python fastmcp/pytest suite that used to live in this
//! directory: the tests observe exactly what an agent observes, over the same
//! client stack (`rmcp`) the host bridge itself is built on.
//!
//! The hostile-input block carries its python module docstring forward: the
//! component exists to run untrusted-input parsers (legacy OLE/CFB, ZIP+XML,
//! RTF) under a capability ceiling, so the interesting property is not "it
//! converts documents" but "it refuses hostile ones cleanly". A panic inside
//! wasm traps and kills the instance, so every hostile case below asserts a
//! structured error — proof the guest returned rather than died.
//!
//! Env: WASM — path to the packed component (default: the component's
//!      release build output);
//!      ACT  — the act invocation (default `act`; `npx @actcore/act`, the
//!             component justfile's default, also works — whitespace-split,
//!             like the shlex.split the python conftest did).

use std::path::{Path, PathBuf};

use rmcp::{ServiceExt, model::CallToolRequestParams, transport::TokioChildProcess};
use serde_json::{Value, json};

/// `().serve(transport)` hands back the client-role service running over the
/// child process: role first, the unit client handler second.
type Client = rmcp::service::RunningService<rmcp::service::RoleClient, ()>;

// ── plumbing ─────────────────────────────────────────────────────────

fn wasm_path() -> PathBuf {
    PathBuf::from(std::env::var("WASM").unwrap_or_else(|_| {
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../target/wasm32-wasip2/release/component_anydoc.wasm"
        )
        .into()
    }))
}

/// The ACT invocation, honouring the same override the component justfile
/// uses. Its default there is `npx @actcore/act` — two words — which cannot
/// be `argv[0]` for a non-shell spawn, so the value is whitespace-split into
/// program + leading args. Quoted paths with spaces are not a form this
/// fleet passes through `ACT`; a full shlex is deliberately not pulled in.
fn act_argv() -> Vec<String> {
    std::env::var("ACT")
        .unwrap_or_else(|_| "act".into())
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// The checked-in document corpus, `e2e/fixtures/`. Fixtures here are static
/// documents, not per-test temp files, so both hosts share them and the
/// grant targets the directory itself.
fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

/// Spawn `act run <wasm> --mcp`, optionally with the read-only
/// `wasi:filesystem` grant on `e2e/fixtures/`.
///
/// Grants are NOT optional: the default policy mode is `ask` and a headless
/// run degrades it to deny. The grant shape is carried verbatim from the
/// python conftest's `granted_client` (mode `allowlist`, `ro`, the `/**`
/// subtree glob). The component's ceiling is `path = "**", mode = "ro"`, so
/// this grants a subset of what is declared, nothing wider.
fn act_command(granted: bool) -> tokio::process::Command {
    let argv = act_argv();
    let mut cmd = tokio::process::Command::new(&argv[0]);
    cmd.args(&argv[1..]);
    cmd.arg("run").arg(wasm_path()).arg("--mcp");
    if granted {
        let grant = json!({
            "wasi:filesystem": {
                "mode": "allowlist",
                "allow": [{
                    "path": format!("{}/**", fixtures_dir().display()),
                    "mode": "ro",
                }],
            }
        });
        cmd.args(["--grant", &grant.to_string()]);
    }
    cmd
}

/// The ungranted host: a `data` source needs no grant at all, so every
/// inline-bytes test runs against this one — and so does the
/// capability-denial assertion, which proves the ceiling actually denies a
/// `path` read when nothing is granted.
async fn connect() -> Client {
    ().serve(TokioChildProcess::new(act_command(false)).expect("spawn act run --mcp"))
        .await
        .expect("rmcp handshake with act run --mcp")
}

/// The granted host: read-only on `e2e/fixtures/`, nothing else. The only way
/// to exercise a `path` source actually succeeding — a denied call never
/// reaches format detection at all, so the ungranted host cannot stand in.
async fn connect_granted() -> Client {
    ().serve(TokioChildProcess::new(act_command(true)).expect("spawn act run --mcp --grant"))
        .await
        .expect("rmcp handshake with act run --mcp --grant")
}

/// Minimal standard-alphabet base64 (RFC 4648, with padding) for the
/// `{"$bytes": "<base64>"}` wrapper the python conftest's `fixture_bytes`
/// fixture built — the transport's canonical byte-string projection for a
/// `data` argument. Hand-rolled rather than pulling the base64 crate into a
/// host-only test crate for one encode direction.
const B64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn b64(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = ((chunk[0] as u32) << 16)
            | ((chunk.get(1).copied().unwrap_or(0) as u32) << 8)
            | (chunk.get(2).copied().unwrap_or(0) as u32);
        out.push(B64_ALPHABET[(n >> 18) as usize & 0x3f] as char);
        out.push(B64_ALPHABET[(n >> 12) as usize & 0x3f] as char);
        out.push(if chunk.len() > 1 {
            B64_ALPHABET[(n >> 6) as usize & 0x3f] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64_ALPHABET[n as usize & 0x3f] as char
        } else {
            '='
        });
    }
    out
}

/// Load a fixture document under `e2e/fixtures/` and wrap it as `data` — the
/// python conftest's `fixture_bytes` fixture.
fn fixture_bytes(name: &str) -> Value {
    let path = fixtures_dir().join(name);
    let raw = std::fs::read(&path).unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()));
    json!({ "$bytes": b64(&raw) })
}

fn structured(result: &rmcp::model::CallToolResult) -> &Value {
    result
        .structured_content
        .as_ref()
        .expect("a successful tool result must carry structured content")
}

/// Call a tool and demand success — the common path every happy-case test
/// starts from.
async fn call_ok(client: &Client, tool: &'static str, args: Value) -> rmcp::model::CallToolResult {
    let params = CallToolRequestParams::new(tool)
        .with_arguments(args.as_object().expect("args object").clone());
    let result = client.call_tool(params).await.expect("call_tool");
    assert_ne!(result.is_error, Some(true), "{tool} failed: {result:?}");
    result
}

/// Assert a call fails with a specific ACT error kind, and optionally a
/// substring of the human-readable error message — the python conftest's
/// `expect_error` fixture.
///
/// Measured, not assumed. `call-tool` in `act:tools` returns a bare
/// `tool-result` with NO `result<>` wrapper — only `list-tools` has one — so
/// a guest reporting a failed tool call can only do it through
/// `tool-event::error`, which arrives as a result with `is_error` set and the
/// kind in `_meta`. That is the path a tool test takes, and on that path the
/// human message lands in `content[0].text`.
///
/// The JSON-RPC error path exists for failures that are not the guest's tool
/// body: `list-tools`, the session operations, a wasmtime trap, an
/// unreachable actor. No tool test in this suite is expected to reach it, but
/// it is handled here so callers need not care.
async fn expect_error(
    client: &Client,
    tool: &'static str,
    args: Value,
    kind: &str,
    message_contains: Option<&str>,
) {
    let params = CallToolRequestParams::new(tool)
        .with_arguments(args.as_object().expect("args object").clone());
    let (got_kind, message) = match client.call_tool(params).await {
        Err(rmcp::ServiceError::McpError(e)) => {
            let got_kind = e
                .data
                .as_ref()
                .and_then(|d| d.get("dev.actcore/error-kind"))
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .expect("JSON-RPC error path must carry dev.actcore/error-kind");
            (got_kind, e.message.to_string())
        }
        Ok(result) => {
            assert_eq!(result.is_error, Some(true), "expected {tool} to fail, got {result:?}");
            let got_kind = result
                .meta
                .as_ref()
                .and_then(|m| m.0.get("dev.actcore/error-kind"))
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .expect("isError path must carry dev.actcore/error-kind");
            let message = result
                .content
                .first()
                .and_then(|b| match b {
                    rmcp::model::ContentBlock::Text(t) => Some(t.text.clone()),
                    _ => None,
                })
                .unwrap_or_default();
            (got_kind, message)
        }
        Err(other) => panic!("unexpected transport failure: {other:?}"),
    };
    assert_eq!(got_kind, kind, "expected {kind} for {tool}, got {got_kind}");
    if let Some(needle) = message_contains {
        assert!(
            message.contains(needle),
            "expected message to contain {needle:?}, got {message:?}"
        );
    }
}

// ── tools (python test_tools.py) ─────────────────────────────────────

#[tokio::test]
async fn lists_the_three_tools() {
    let client = connect().await;
    let tools = client.list_all_tools().await.expect("list_all_tools");
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    assert_eq!(tools.len(), 3, "expected exactly three tools, got: {names:?}");
    assert!(names.contains(&"convert"), "convert must be among the tools, got: {names:?}");
    assert!(names.contains(&"detect"), "detect must be among the tools, got: {names:?}");
    assert!(
        names.contains(&"extract_assets"),
        "extract_assets must be among the tools, got: {names:?}"
    );
    client.cancel().await.ok();
}

// ── manifest (python test_info.py) ───────────────────────────────────

/// The manifest probe: the packed artifact must declare its name and a
/// version. Also the fast-fail the python `wasm_path` fixture provided — an
/// unpacked wasm (raw `cargo build` output, no `act:component` section)
/// declares no ceiling, every grant is refused as "outside ceiling", and the
/// failures point anywhere but at the missing metadata. The justfile's
/// `test: build` ordering exists so this test finds a packed artifact.
#[test]
fn manifest_reports_name_and_version() {
    let output = {
        let argv = act_argv();
        let mut cmd = std::process::Command::new(&argv[0]);
        cmd.args(&argv[1..]);
        cmd.args(["inspect", "component-manifest"])
            .arg(wasm_path())
            .output()
            .expect("run act inspect component-manifest")
    };
    assert!(
        output.status.success(),
        "inspect failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let manifest: Value = serde_json::from_slice(&output.stdout).expect("manifest is JSON");
    assert_eq!(
        manifest["std"]["name"], "anydoc",
        "packed manifest must carry the component name"
    );
    assert!(
        manifest["std"]["version"].is_string(),
        "packed manifest must carry a version, got: {}",
        manifest["std"]["version"]
    );
}

// ── convert (python test_convert.py) ─────────────────────────────────

/// (fixture, extra args, expected format, markdown substrings that must
/// appear). The inventory.xlsx case is the only one exercising calamine — an
/// entirely separate parser stack from the OOXML/docx path — and its sheet
/// renders as a GFM table. PDF makes the component a one-stop "any document
/// to Markdown" tool rather than one that routes by format.
///
/// A function, not a `const`: `json!` is not const-callable.
fn conversions() -> Vec<(&'static str, Value, &'static str, &'static [&'static str])> {
    vec![
        ("report.docx", json!({}), "docx", &["Quarterly Report", "twelve percent"]),
        ("note.rtf", json!({}), "rtf", &["RTF note"]),
        ("inventory.xlsx", json!({}), "excel", &["| Part | Qty |", "bolt"]),
        // CSV carries no signature, so naming the format explicitly is all it needs.
        ("parts.csv", json!({"format": "csv"}), "csv", &["bolt"]),
        ("leaflet.pdf", json!({}), "pdf", &["leaflet"]),
    ]
}

#[tokio::test]
async fn converts_to_markdown() {
    let client = connect().await;
    for (filename, extra_args, expected_format, substrings) in conversions() {
        let mut args = json!({"data": fixture_bytes(filename)});
        for (k, v) in extra_args.as_object().expect("extra args object") {
            args[k] = v.clone();
        }
        let result = call_ok(&client, "convert", args).await;
        let data = structured(&result);
        assert_eq!(
            data["format"], expected_format,
            "{filename}: wrong parser chosen"
        );
        let markdown = data["markdown"].as_str().expect("markdown is a string");
        for substring in substrings {
            assert!(
                markdown.contains(substring),
                "{filename}: expected {substring:?} in the markdown, got: {markdown:?}"
            );
        }
    }
    client.cancel().await.ok();
}

#[tokio::test]
async fn csv_with_no_hint_is_rejected() {
    let client = connect().await;
    // CSV carries no signature, so with no hint there is nothing to detect.
    expect_error(
        &client,
        "convert",
        json!({"data": fixture_bytes("parts.csv")}),
        "std:invalid-args",
        Some("format"),
    )
    .await;
    client.cancel().await.ok();
}

#[tokio::test]
async fn explicit_format_overrides_detection_and_fails_at_the_parser() {
    let client = connect().await;
    // An explicit format is a caller assertion and overrides detection — so
    // asserting the wrong one fails at the parser rather than being
    // second-guessed.
    expect_error(
        &client,
        "convert",
        json!({"data": fixture_bytes("report.docx"), "format": "rtf"}),
        "std:invalid-args",
        None,
    )
    .await;
    client.cancel().await.ok();
}

// ── detect (python test_detect.py) ───────────────────────────────────

/// (fixture, extra args, expected format, expected detected_from).
fn detections() -> Vec<(&'static str, Value, &'static str, &'static str)> {
    vec![
        ("report.docx", json!({}), "docx", "content"),
        // The same bytes under a lying filename. Content wins — this is the
        // case the tool exists for.
        ("report.docx", json!({"filename": "innocent.txt"}), "docx", "content"),
        // CSV has no signature, so the extension is all there is.
        ("parts.csv", json!({"filename": "parts.csv"}), "csv", "extension"),
    ]
}

#[tokio::test]
async fn detects_format() {
    let client = connect().await;
    for (filename, extra_args, expected_format, expected_from) in detections() {
        let mut args = json!({"data": fixture_bytes(filename)});
        for (k, v) in extra_args.as_object().expect("extra args object") {
            args[k] = v.clone();
        }
        let result = call_ok(&client, "detect", args).await;
        let data = structured(&result);
        assert_eq!(data["format"], expected_format, "{filename}: wrong format");
        assert_eq!(
            data["detected_from"], expected_from,
            "{filename}: wrong provenance"
        );
    }
    client.cancel().await.ok();
}

#[tokio::test]
async fn no_signature_and_no_filename_is_rejected() {
    let client = connect().await;
    // No signature and no filename: nothing to go on.
    expect_error(
        &client,
        "detect",
        json!({"data": fixture_bytes("parts.csv")}),
        "std:invalid-args",
        None,
    )
    .await;
    client.cancel().await.ok();
}

#[tokio::test]
async fn path_source_reads_and_detects_under_a_grant() {
    // The same property `detects_format` checks, but through `path` on a real
    // file sitting on disk under a lying .txt name, against a host granted
    // read access to e2e/fixtures/. This is the only case that reads via
    // `path` successfully — every other `path` case (the hardening suite)
    // asserts the capability-denied ceiling instead, since a denied call
    // never reaches format detection at all.
    let client = connect_granted().await;
    let result = call_ok(
        &client,
        "detect",
        json!({"path": fixtures_dir().join("mislabeled.txt").display().to_string()}),
    )
    .await;
    let data = structured(&result);
    assert_eq!(data["format"], "docx");
    assert_eq!(data["detected_from"], "content");
    client.cancel().await.ok();
}

// ── extract_assets (python test_assets.py) ───────────────────────────

#[tokio::test]
async fn single_image_returns_manifest_and_content_part() {
    let client = connect().await;
    let result = call_ok(&client, "extract_assets", json!({"data": fixture_bytes("with-image.docx")})).await;
    // Multi-part result: manifest + one image part, so structured_content is
    // unpopulated (measured — only a single part decoding to a JSON object
    // gets one) and the manifest is read back from content[0].text instead.
    assert!(
        result.structured_content.is_none(),
        "a multi-part result must not carry structured content, got: {:?}",
        result.structured_content
    );
    let manifest: Value =
        serde_json::from_str(&first_text(&result)).expect("manifest is JSON");
    let assets = manifest["assets"].as_array().expect("assets is a list");
    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0]["media_type"], "image/png");
    assert_eq!(assets[0]["id"], 0);
    assert!(
        assets[0]["origin_part"].as_str().expect("origin_part is a string").contains("image1.png"),
        "origin_part must name the source part, got: {}",
        assets[0]["origin_part"]
    );
    assert_eq!(mime_of(&result.content[1]), "image/png");
    client.cancel().await.ok();
}

#[tokio::test]
async fn selecting_by_id_on_a_single_asset_document() {
    let client = connect().await;
    // Selecting by id returns the manifest plus only the chosen asset. With a
    // single-asset document this cannot distinguish "filtered" from "ids was
    // ignored" — the two-image cases below do that.
    let result = call_ok(
        &client,
        "extract_assets",
        json!({"data": fixture_bytes("with-image.docx"), "ids": [0]}),
    )
    .await;
    let manifest: Value =
        serde_json::from_str(&first_text(&result)).expect("manifest is JSON");
    assert_eq!(manifest["assets"].as_array().expect("assets is a list").len(), 1);
    assert_eq!(mime_of(&result.content[1]), "image/png");
    client.cancel().await.ok();
}

#[tokio::test]
async fn two_images_unfiltered_returns_both_in_asset_id_order() {
    let client = connect().await;
    // Two embedded images of different media types, unfiltered: the manifest
    // lists both, and both content parts follow in asset-id order.
    let result = call_ok(&client, "extract_assets", json!({"data": fixture_bytes("two-images.docx")})).await;
    assert_eq!(result.content.len(), 3);
    let manifest: Value =
        serde_json::from_str(&first_text(&result)).expect("manifest is JSON");
    assert_eq!(manifest["assets"].as_array().expect("assets is a list").len(), 2);
    assert_eq!(mime_of(&result.content[1]), "image/png");
    assert_eq!(mime_of(&result.content[2]), "image/gif");
    client.cancel().await.ok();
}

#[tokio::test]
async fn filtering_by_id_excludes_the_other_asset() {
    let client = connect().await;
    // Filtering by `ids: [1]` on the same two-image document: the manifest
    // still lists both assets (it always lists everything), but only the
    // id-1 part (the GIF) follows — asserting the total content length is
    // what actually proves the PNG was excluded rather than merely
    // unasserted.
    let result = call_ok(
        &client,
        "extract_assets",
        json!({"data": fixture_bytes("two-images.docx"), "ids": [1]}),
    )
    .await;
    assert_eq!(result.content.len(), 2);
    let manifest: Value =
        serde_json::from_str(&first_text(&result)).expect("manifest is JSON");
    assert_eq!(manifest["assets"].as_array().expect("assets is a list").len(), 2);
    assert_eq!(mime_of(&result.content[1]), "image/gif");
    client.cancel().await.ok();
}

#[tokio::test]
async fn document_with_no_assets_returns_an_empty_manifest() {
    let client = connect().await;
    // A document with no embedded assets returns an empty manifest, not an
    // error.
    let result = call_ok(&client, "extract_assets", json!({"data": fixture_bytes("report.docx")})).await;
    let assets = structured(&result)["assets"].as_array().expect("assets is a list");
    assert!(assets.is_empty(), "expected an empty manifest, got: {assets:?}");
    client.cancel().await.ok();
}

#[tokio::test]
async fn pdf_is_refused_with_a_pointer_to_pdf_inspector() {
    let client = connect().await;
    // PDF has no document model upstream, so it is refused — with a pointer
    // to the component that can do the job.
    expect_error(
        &client,
        "extract_assets",
        json!({"data": fixture_bytes("leaflet.pdf")}),
        "std:invalid-args",
        Some("pdf-inspector"),
    )
    .await;
    client.cancel().await.ok();
}

// ── hostile input (python test_hardening.py) ─────────────────────────

/// (tool, fixture). `truncated.docx` is a valid ZIP magic with the archive
/// cut in half; `garbage.bin` is a ZIP header followed by 4 KiB of noise;
/// `empty.bin` is empty input. `detect` must survive the same inputs.
///
/// One fresh `act` process per case: the python conftest's function-scoped
/// `client` fixture gave every parametrized case its own instance, and a
/// case that traps (the regression this suite exists to catch) must not
/// poison the cases after it.
#[tokio::test]
async fn rejects_hostile_bytes() {
    for (tool, fixture_file) in [
        ("convert", "truncated.docx"),
        ("convert", "garbage.bin"),
        ("convert", "empty.bin"),
        ("detect", "garbage.bin"),
        ("extract_assets", "truncated.docx"),
    ] {
        let client = connect().await;
        expect_error(
            &client,
            tool,
            json!({"data": fixture_bytes(fixture_file)}),
            "std:invalid-args",
            None,
        )
        .await;
        client.cancel().await.ok();
    }
}

#[tokio::test]
async fn unbalanced_rtf_groups_are_recovered_not_rejected() {
    let client = connect().await;
    // note.rtf cut in half, mid font-table entry: 3 open groups, 0 closes at
    // EOF. anydoc's RTF parser treats unbalanced groups as recoverable
    // (logged, not rejected) rather than a hard error, so this is a clean
    // success with empty output, not an error — a different but equally
    // valid "did not trap" outcome. Verified against the running component
    // before asserting.
    let result = call_ok(
        &client,
        "convert",
        json!({"data": fixture_bytes("truncated-note.rtf")}),
    )
    .await;
    let data = structured(&result);
    assert_eq!(data["format"], "rtf");
    assert_eq!(data["markdown"], "");
    client.cancel().await.ok();
}

#[tokio::test]
async fn xml_depth_safety_limit_fires_with_its_own_error_kind() {
    let client = connect().await;
    // word/document.xml nests a chain ~300 levels deep — past anydoc's
    // MAX_XML_DEPTH (256), well-formed XML so it is the depth cap that fires,
    // not a parse error. This is the one error kind the component's sandbox
    // claim actually rests on (the fixed caps on entry/archive size, entry
    // count, XML depth and node count), so it gets its own end-to-end case
    // rather than staying unit-tested only in src/error.rs.
    //
    // `anydoc:resource-limit` is a custom (non-`std:`) kind — ACT-SPEC
    // permits namespaced kinds and requires hosts not to reject
    // unrecognised ones, and this is genuinely a structured error, not a
    // papered-over trap.
    expect_error(
        &client,
        "convert",
        json!({"data": fixture_bytes("deep-nest.docx")}),
        "anydoc:resource-limit",
        Some("limit"),
    )
    .await;
    client.cancel().await.ok();
}

#[tokio::test]
async fn rejects_when_neither_source_supplied() {
    let client = connect().await;
    // Neither source supplied.
    expect_error(&client, "convert", json!({}), "std:invalid-args", Some("data")).await;
    client.cancel().await.ok();
}

#[tokio::test]
async fn rejects_when_both_sources_supplied() {
    let client = connect().await;
    // Both supplied — ambiguous, so rejected rather than silently preferring
    // one.
    expect_error(
        &client,
        "convert",
        json!({"data": fixture_bytes("report.docx"), "path": "/tmp/x.docx"}),
        "std:invalid-args",
        Some("not both"),
    )
    .await;
    client.cancel().await.ok();
}

#[tokio::test]
async fn path_source_with_no_grant_is_denied() {
    let client = connect().await;
    // A `path` source with no filesystem grant must be denied, not served.
    // The e2e host runs headless with no grant, so ask-by-default degrades
    // to deny and the component cannot read the file even though it exists.
    // The relative path resolves against the host's working directory, which
    // is this crate — `fixtures/report.docx` is the checked-in corpus.
    expect_error(
        &client,
        "convert",
        json!({"path": "fixtures/report.docx"}),
        "std:capability-denied",
        None,
    )
    .await;
    client.cancel().await.ok();
}

#[tokio::test]
async fn data_source_needs_no_grant() {
    let client = connect().await;
    // The same document as `data` needs no grant and succeeds — the ceiling
    // constrains the filesystem path, not the component's core function.
    let result = call_ok(
        &client,
        "convert",
        json!({"data": fixture_bytes("report.docx")}),
    )
    .await;
    let data = structured(&result);
    assert_eq!(data["format"], "docx");
    client.cancel().await.ok();
}

// ── content-block helpers ────────────────────────────────────────────

fn first_text(result: &rmcp::model::CallToolResult) -> String {
    match result.content.first() {
        Some(rmcp::model::ContentBlock::Text(t)) => t.text.clone(),
        other => panic!("expected the first content block to be Text, got: {other:?}"),
    }
}

/// The MIME type of a content block — an image block's `mimeType` on the
/// wire, the thing the python suite read off `result.content[i]`.
fn mime_of(block: &rmcp::model::ContentBlock) -> &str {
    match block {
        rmcp::model::ContentBlock::Image(img) => &img.mime_type,
        other => panic!("expected an Image content block, got: {other:?}"),
    }
}
