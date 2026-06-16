.PHONY: build install release test ci clippy deb

VERSION := $(shell grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
ARCH := $(shell uname -m)

build:
	cargo build --release

install:
	cargo install --path .

deb: build
	mkdir -p pkg/deb/usr/local/bin
	cp target/release/mso pkg/deb/usr/local/bin/mso
	mkdir -p pkg/deb/DEBIAN
	printf "Package: mso\nVersion: $(VERSION)\nArchitecture: amd64\nMaintainer: MSO Contributors\nDescription: Multi-Stream Orchestrator\n" > pkg/deb/DEBIAN/control
	dpkg-deb --build pkg/deb mso_$(VERSION)_$(ARCH).deb
	rm -rf pkg

release: build
	gh release create v$(VERSION) \
	  --title "MSO v$(VERSION)" \
	  ./target/release/mso#mso-x86_64-unknown-linux-gnu

upload-release: deb
	gh release upload v$(VERSION) mso_$(VERSION)_*.deb

test:
	cargo test --lib -- daemon::signal::tests daemon::log_db::tests daemon::telemetry::tests protocol::tests
	cargo test --test integration
	cargo test --test tui_snapshot

ci: clippy test build

clippy:
	cargo clippy -- -D warnings
