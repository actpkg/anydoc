# A real docx identifies itself by signature.
POST {{baseurl}}/tools/detect
Content-Type: application/json
file,fixtures/args/report.json;
HTTP 200
[Asserts]
jsonpath "$.content[0].data.format" == "docx"
jsonpath "$.content[0].data.detected_from" == "content"

# The same bytes under a lying filename. Content wins — this is the case the
# tool exists for.
POST {{baseurl}}/tools/detect
Content-Type: application/json
file,fixtures/args/mislabeled-named.json;
HTTP 200
[Asserts]
jsonpath "$.content[0].data.format" == "docx"
jsonpath "$.content[0].data.detected_from" == "content"

# CSV has no signature, so the extension is all there is.
POST {{baseurl}}/tools/detect
Content-Type: application/json
file,fixtures/args/parts-named.json;
HTTP 200
[Asserts]
jsonpath "$.content[0].data.format" == "csv"
jsonpath "$.content[0].data.detected_from" == "extension"

# No signature and no filename: nothing to go on.
POST {{baseurl}}/tools/detect
Content-Type: application/json
file,fixtures/args/parts.json;
HTTP 422
[Asserts]
jsonpath "$.error.kind" == "std:invalid-args"

# The same property again, but through `path` on a real file sitting on disk
# under a lying .txt name, against the host granted read access to
# e2e/fixtures/. This is the only e2e case that reads via `path`
# successfully — every other `path` case (hardening.hurl) asserts the
# capability-denied ceiling instead, since a denied call never reaches
# format detection at all.
POST {{gbaseurl}}/tools/detect
Content-Type: application/json
{"arguments": {"path": "{{fixtures_dir}}/mislabeled.txt"}}
HTTP 200
[Asserts]
jsonpath "$.content[0].data.format" == "docx"
jsonpath "$.content[0].data.detected_from" == "content"
