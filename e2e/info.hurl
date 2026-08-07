GET {{baseurl}}/info
HTTP 200
[Asserts]
jsonpath "$.std.name" == "anydoc"
jsonpath "$.std.version" isString
