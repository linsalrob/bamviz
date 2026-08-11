.PHONY: check

check:
	cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace
	cd web && npm ci && npm run check && npm run test && npm run build
