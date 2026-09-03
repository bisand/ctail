# Top-level convenience targets. The real build logic lives in:
#   core/    Rust engine crate (cargo)
#   macos/   native macOS app (SwiftPM / xcodegen)  -> see macos/Makefile
#   site/    website (SvelteKit)                    -> see site/README.md
#   legacy/wails/  archived Go + Svelte app          -> see legacy/wails/README.md

.PHONY: all test test-core test-macos bench-core fmt lint clean

all: test

test: test-core test-macos

test-core:
	cargo test -p ctail-core --all-features
	cargo build -p ctail-desktop

test-macos:
	$(MAKE) -C macos test

# Tail-engine benchmark. Example: make bench-core ARGS="--gen 2G --cold"
bench-core:
	cargo run --release -p ctail-core --example tailbench -- $(ARGS)

fmt:
	cargo fmt --all

lint:
	cargo fmt --all -- --check
	cargo clippy -p ctail-core --all-targets --all-features -- -D warnings
	cargo clippy -p ctail-desktop -- -D warnings

clean:
	cargo clean
	$(MAKE) -C macos clean
