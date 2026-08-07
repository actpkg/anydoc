# Hostile-input suite.
#
# This component exists to run untrusted-input parsers under a capability
# ceiling, so the interesting property is not "it converts documents" but "it
# refuses hostile ones cleanly". A panic inside wasm traps and kills the
# instance, so every case below asserts a structured error — proof the guest
# returned rather than died. An *unexpected* 500 with no structured `error`
# body is that failure signature. One case below (the XML-depth safety
# limit) legitimately expects 500 for a documented, non-trap reason — see
# its comment.

# Valid ZIP magic, archive cut in half.
POST {{baseurl}}/tools/convert
Content-Type: application/json
file,fixtures/args/truncated.json;
HTTP 422
[Asserts]
jsonpath "$.error.kind" == "std:invalid-args"

# ZIP header followed by 4 KiB of noise.
POST {{baseurl}}/tools/convert
Content-Type: application/json
file,fixtures/args/garbage.json;
HTTP 422
[Asserts]
jsonpath "$.error.kind" == "std:invalid-args"

# Empty input.
POST {{baseurl}}/tools/convert
Content-Type: application/json
file,fixtures/args/empty.json;
HTTP 422
[Asserts]
jsonpath "$.error.kind" == "std:invalid-args"

# detect must survive the same inputs.
POST {{baseurl}}/tools/detect
Content-Type: application/json
file,fixtures/args/garbage.json;
HTTP 422
[Asserts]
jsonpath "$.error.kind" == "std:invalid-args"

POST {{baseurl}}/tools/extract_assets
Content-Type: application/json
file,fixtures/args/truncated.json;
HTTP 422
[Asserts]
jsonpath "$.error.kind" == "std:invalid-args"

# note.rtf cut in half, mid font-table entry: 3 open groups, 0 closes at
# EOF. anydoc's RTF parser treats unbalanced groups as recoverable (logged,
# not rejected) rather than a hard error, so this is a clean 200 with empty
# output, not a 422 — a different but equally valid "did not trap" outcome.
# Verified against the running component before asserting.
POST {{baseurl}}/tools/convert
Content-Type: application/json
file,fixtures/args/truncated-note.json;
HTTP 200
[Asserts]
jsonpath "$.content[0].data.format" == "rtf"
jsonpath "$.content[0].data.markdown" == ""

# ── Safety limits ────────────────────────────────────────────────────

# word/document.xml nests a chain ~300 levels deep — past anydoc's
# MAX_XML_DEPTH (256), well-formed XML so it is the depth cap that fires,
# not a parse error. This is the one error kind the component's sandbox
# claim actually rests on (the fixed caps on entry/archive size, entry
# count, XML depth and node count), so it gets its own end-to-end case
# rather than staying unit-tested only in src/error.rs.
#
# `anydoc:resource-limit` is a custom (non-`std:`) kind. ACT-HTTP only maps
# the well-known `std:*` kinds to specific statuses and explicitly leaves
# custom kinds up to the server (ACT-HTTP.md §6.1, "servers MAY map custom
# kinds..."); act-cli's fallback for an unmapped kind is 500. So this
# genuinely is 500 by design, not a papered-over trap — the body is a clean
# structured error (`{"error": {"kind": "anydoc:resource-limit", ...}}`),
# not a wasmtime trap dump.
POST {{baseurl}}/tools/convert
Content-Type: application/json
file,fixtures/args/deep-nest.json;
HTTP 500
[Asserts]
jsonpath "$.error.kind" == "anydoc:resource-limit"
jsonpath "$.error.message" contains "limit"

# ── Argument validation ──────────────────────────────────────────────

# Neither source supplied.
POST {{baseurl}}/tools/convert
Content-Type: application/json
file,fixtures/args/no-source.json;
HTTP 422
[Asserts]
jsonpath "$.error.kind" == "std:invalid-args"
jsonpath "$.error.message" contains "data"

# Both supplied — ambiguous, so rejected rather than silently preferring one.
POST {{baseurl}}/tools/convert
Content-Type: application/json
file,fixtures/args/both-sources.json;
HTTP 422
[Asserts]
jsonpath "$.error.kind" == "std:invalid-args"
jsonpath "$.error.message" contains "not both"

# ── Capability ceiling ───────────────────────────────────────────────

# A `path` source with no filesystem grant must be denied, not served. The e2e
# host runs headless with no grant, so ask-by-default degrades to deny and the
# component cannot read the file even though it exists.
POST {{baseurl}}/tools/convert
Content-Type: application/json
{"arguments": {"path": "fixtures/report.docx"}}
HTTP 403
[Asserts]
jsonpath "$.error.kind" == "std:capability-denied"

# The same document as `data` needs no grant and succeeds — the ceiling
# constrains the filesystem path, not the component's core function.
POST {{baseurl}}/tools/convert
Content-Type: application/json
file,fixtures/args/report.json;
HTTP 200
[Asserts]
jsonpath "$.content[0].data.format" == "docx"
