# Archived: the Wails (Go + Svelte) ctail app

This directory holds the original cross-platform ctail implementation — a
[Wails v2](https://wails.io/) app with a Go backend and a Svelte frontend — as it
was when development moved to the native macOS app (`macos/`) and the shared
Rust engine (`core/`).

It is kept for reference and is **not built by CI anymore**. Its GitHub Actions
workflows live in `workflows/` here (moved out of `.github/workflows/` so they
no longer run) and its Go module root is this directory:

```bash
cd legacy/wails
make dev      # wails dev
make test     # go test ./internal/...
```

The macOS app still reads its theme catalogue from
`internal/config/themes.go` via `make -C macos themes`, so that file stays the
source of truth until themes move into `core/`.
