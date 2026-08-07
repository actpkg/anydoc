# Hostile-input suite.
#
# This component exists to run untrusted-input parsers under a capability
# ceiling, so the interesting property is not "it converts documents" but "it
# refuses hostile ones cleanly". A panic inside wasm traps and kills the
# instance, so every case below asserts a structured error — proof the guest
# returned rather than died. A trap surfaces as a 500 and fails these tests.

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
