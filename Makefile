.PHONY: build install release test ci clippy

build:
	cargo build --release

install:
	cargo install --path .

release:
	gh release create v$(shell grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/') \
	  --title "MSO v$(shell grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/")" \
	  ./target/release/mso#mso-x86_64-unknown-linux-gnu

test:
	cargo test --lib -- daemon::signal::tests daemon::log_db::tests daemon::telemetry::tests protocol::tests
	cargo test --test integration
	cargo test --test tui_snapshot

ci: clippy test build

clippy:
	cargo clippy -- -D warnings
