# A docx with one embedded PNG: a manifest, then the image itself.
POST {{baseurl}}/tools/extract_assets
Content-Type: application/json
file,fixtures/args/with-image.json;
HTTP 200
[Asserts]
jsonpath "$.content[0].data.assets" count == 1
jsonpath "$.content[0].data.assets[0].media_type" == "image/png"
jsonpath "$.content[0].data.assets[0].id" == 0
jsonpath "$.content[0].data.assets[0].origin_part" contains "image1.png"
jsonpath "$.content[1].mime_type" == "image/png"

# Selecting by id returns the manifest plus only the chosen asset.
POST {{baseurl}}/tools/extract_assets
Content-Type: application/json
file,fixtures/args/with-image-id0.json;
HTTP 200
[Asserts]
jsonpath "$.content[0].data.assets" count == 1
jsonpath "$.content[1].mime_type" == "image/png"

# A document with no embedded assets returns an empty manifest, not an error.
POST {{baseurl}}/tools/extract_assets
Content-Type: application/json
file,fixtures/args/report.json;
HTTP 200
[Asserts]
jsonpath "$.content[0].data.assets" count == 0

# PDF has no document model upstream, so it is refused — with a pointer to the
# component that can do the job.
POST {{baseurl}}/tools/extract_assets
Content-Type: application/json
file,fixtures/args/leaflet.json;
HTTP 422
[Asserts]
jsonpath "$.error.kind" == "std:invalid-args"
jsonpath "$.error.message" contains "pdf-inspector"
