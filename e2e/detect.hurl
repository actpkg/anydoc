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
