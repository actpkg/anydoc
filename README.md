# anydoc

An [ACT](https://actcore.dev) component that converts Word, PowerPoint, Excel,
OpenDocument, RTF, EPUB, CSV and PDF documents to GitHub-Flavored Markdown.

Wraps [`anydoc`](https://github.com/firecrawl/anydoc) (MIT).

## Why a sandbox

This runs legacy OLE/CFB binary parsers, ZIP+XML package readers and an RTF
parser over input you did not write — a long-standing memory-corruption and
decompression-bomb surface.

The component's declared ceiling is **read-only `wasi:filesystem` and nothing
else**. `wasi:http` and `wasi:sockets` are absent, so they are denied
unconditionally: a weaponised document that fully corrupts the parser still
cannot open a socket or write a byte. Documents passed inline as `data` need
no grant at all.

## Artifact size

The packed component is about 6 MB (~5.7 MiB) — large for a component,
because it bundles parsers for 14 document formats. Callers pulling it over
OCI should budget for that.

## Usage

```bash
# Inline bytes — no grant needed.
act call actpkg.dev/library/anydoc:0.1.0 convert \
  --args "{\"data\": {\"\$bytes\": \"$(base64 -w0 report.docx)\"}}"

# From disk — needs a read grant.
act call actpkg.dev/library/anydoc:0.1.0 convert \
  --args '{"path": "/data/report.docx"}' \
  --grant '{"wasi:filesystem":{"mode":"allowlist","allow":[{"path":"/data/**","mode":"ro"}]}}'
```

## Development

```bash
just init        # fetch WIT deps
just build       # cargo build --release (wasm32-wasip2)
just test-unit   # fast host-target unit tests
just pack        # embed act:component + act:skill
just test        # e2e: act run --http + hurl
```

Requires the nightly toolchain pinned in `rust-toolchain.toml` — see the
comment there for why, and for the condition under which it can go back to
stable.

## Publishing

Pushing to `main` publishes a signed component to
`actpkg.dev/<owner>/anydoc` (owner derived from the git remote;
override the full path with the `OCI_REGISTRY` env var). CI signs the image
keylessly with [cosign](https://docs.sigstore.dev/) via GitHub OIDC.

One-time setup: create a Personal Access Token at
[actpkg.dev](https://actpkg.dev) and add it as a repository secret named
**`ACTPKG_TOKEN`** (Settings → Secrets and variables → Actions).

```bash
just publish   # local publish (unsigned); CI signs on push to main
```

## License

MIT OR Apache-2.0
