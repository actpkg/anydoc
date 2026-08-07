# Convert a real WordprocessingML document.
POST {{baseurl}}/tools/convert
Content-Type: application/json
file,fixtures/args/report.json;
HTTP 200
[Asserts]
jsonpath "$.content[0].data.format" == "docx"
jsonpath "$.content[0].data.markdown" contains "Quarterly Report"
jsonpath "$.content[0].data.markdown" contains "twelve percent"

# RTF, detected from its signature.
POST {{baseurl}}/tools/convert
Content-Type: application/json
file,fixtures/args/note.json;
HTTP 200
[Asserts]
jsonpath "$.content[0].data.format" == "rtf"
jsonpath "$.content[0].data.markdown" contains "RTF note"

# A real .xlsx, detected from its signature — calamine is an entirely
# separate parser stack from the OOXML/docx path, so this is the only case
# that exercises it. The sheet renders as a GFM table.
POST {{baseurl}}/tools/convert
Content-Type: application/json
file,fixtures/args/inventory.json;
HTTP 200
[Asserts]
jsonpath "$.content[0].data.format" == "excel"
jsonpath "$.content[0].data.markdown" contains "| Part | Qty |"
jsonpath "$.content[0].data.markdown" contains "bolt"

# CSV carries no signature, so with no hint there is nothing to detect.
POST {{baseurl}}/tools/convert
Content-Type: application/json
file,fixtures/args/parts.json;
HTTP 422
[Asserts]
jsonpath "$.error.kind" == "std:invalid-args"
jsonpath "$.error.message" contains "format"

# Naming the format explicitly is all it needs.
POST {{baseurl}}/tools/convert
Content-Type: application/json
file,fixtures/args/parts-typed.json;
HTTP 200
[Asserts]
jsonpath "$.content[0].data.format" == "csv"
jsonpath "$.content[0].data.markdown" contains "bolt"

# PDF too — this is what makes the component a one-stop "any document to
# Markdown" tool rather than one that routes by format.
POST {{baseurl}}/tools/convert
Content-Type: application/json
file,fixtures/args/leaflet.json;
HTTP 200
[Asserts]
jsonpath "$.content[0].data.format" == "pdf"
jsonpath "$.content[0].data.markdown" contains "leaflet"

# An explicit format is a caller assertion and overrides detection — so
# asserting the wrong one fails at the parser rather than being second-guessed.
POST {{baseurl}}/tools/convert
Content-Type: application/json
file,fixtures/args/report-wrong-format.json;
HTTP 422
[Asserts]
jsonpath "$.error.kind" == "std:invalid-args"
