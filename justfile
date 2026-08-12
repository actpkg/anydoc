wasm := "target/wasm32-wasip2/release/component_anydoc.wasm"
# OCI reference to publish to (registry/namespace/name, no tag). Override with OCI_REF.
component_ref := env("OCI_REF", "actpkg.dev/library/anydoc")

act := env("ACT", "npx @actcore/act")
actbuild := env("ACT_BUILD", "npx @actcore/act-build")
hurl := env("HURL", "hurl")
# Two ports, both in a safe range: above the well-known/common dev ports and
# below the Linux outbound ephemeral range (32768+). One host runs
# ungranted, the other granted read-only to e2e/fixtures/. See `test`.
port := `shuf -i 10000-19999 -n 1`
port2 := `shuf -i 20000-29999 -n 1`
addr := "[::1]:" + port
baseurl := "http://" + addr
gaddr := "[::1]:" + port2
gbaseurl := "http://" + gaddr
fixtures_dir := justfile_directory() + "/e2e/fixtures"

# Fetch WIT deps from the registry (ghcr.io/actcore) into wit/deps/.
# wkg-registry.toml maps the act namespace -> actcore.dev (well-known -> ghcr.io/actcore).
init:
    WKG_CONFIG_FILE=wkg-registry.toml wkg wit fetch --type wit

setup: init
    prek install

# Build and pack. Packing is part of building on purpose: `cargo build` alone
# produces a wasm with no `act:component` section, which declares no capability
# ceiling, so at runtime every grant is refused as "outside ceiling" and the
# failure points anywhere but at the missing metadata.
build:
    cargo build --release
    {{actbuild}} pack {{wasm}}

# Fast unit tests for the SDK-free modules, on the host target.
test-unit:
    cargo test --target x86_64-unknown-linux-gnu

# Re-embed act:component metadata and act:skill without rebuilding. `pack` is
# idempotent, so running it after `build` is harmless.
pack:
    {{actbuild}} pack {{wasm}}

test: build
    #!/usr/bin/env bash
    set -euo pipefail
    # Two hosts. The ungranted one proves the capability ceiling actually
    # denies; the granted one is the only way to test a `path` source
    # actually succeeding, since a denied call never reaches format
    # detection at all.
    {{act}} run {{wasm}} --http --listen "{{addr}}" &
    UNGRANTED=$!
    {{act}} run {{wasm}} --http --listen "{{gaddr}}" \
      --grant '{"wasi:filesystem":{"mode":"allowlist","allow":[{"path":"{{fixtures_dir}}/**","mode":"ro"}]}}' &
    GRANTED=$!
    trap "kill $UNGRANTED $GRANTED 2>/dev/null || true" EXIT
    curl --retry 60 --retry-connrefused --retry-delay 1 -fsS -o /dev/null {{baseurl}}/info
    curl --retry 60 --retry-connrefused --retry-delay 1 -fsS -o /dev/null {{gbaseurl}}/info
    {{hurl}} --test --variable "baseurl={{baseurl}}" --variable "gbaseurl={{gbaseurl}}" \
      --variable "fixtures_dir={{fixtures_dir}}" e2e/*.hurl

publish: build
    #!/usr/bin/env bash
    set -euo pipefail
    INFO=$({{act}} inspect component-manifest {{wasm}})
    VERSION=$(echo "$INFO" | jq -r .std.version)
    OUTPUT=$({{actbuild}} push {{wasm}} "{{component_ref}}:$VERSION" \
      --skip-if-exists \
      --also-tag latest 2>&1) || { echo "$OUTPUT" >&2; exit 1; }
    echo "$OUTPUT"
    DIGEST=$(echo "$OUTPUT" | grep "^Digest:" | awk '{print $2}' || true)
    if [ -n "${GITHUB_OUTPUT:-}" ]; then
      echo "image={{component_ref}}" >> "$GITHUB_OUTPUT"
      echo "digest=$DIGEST" >> "$GITHUB_OUTPUT"
    fi
