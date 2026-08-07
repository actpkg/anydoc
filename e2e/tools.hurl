POST {{baseurl}}/tools
HTTP 200
[Asserts]
jsonpath "$.tools" count == 3
jsonpath "$.tools[*].name" contains "convert"
jsonpath "$.tools[*].name" contains "detect"
jsonpath "$.tools[*].name" contains "extract_assets"
