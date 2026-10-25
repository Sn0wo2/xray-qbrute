.PHONY: all fmt fmt-check check check-all-features check-no-default lint lint-fix test test-all test-no-default doc security ci build publish-dry release release-snapshot clean

ci: lint check check-no-default test test-all test-no-default doc security

all: ci

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

check:
	cargo check --workspace --all-targets $(CARGO_FLAGS)

check-all-features:
	cargo check --workspace --all-targets --all-features

# xray-qbrute-core requires at least one backend feature, so the minimal checks keep scalar enabled.
NO_DEFAULT := --no-default-features --features scalar

check-no-default:
	cargo check --workspace --all-targets $(NO_DEFAULT)

lint-fix: fmt
	cargo clippy --workspace --all-targets --all-features --fix --allow-dirty --allow-staged

lint: fmt-check
	cargo clippy --workspace --all-targets --all-features $(CARGO_FLAGS) -- -D warnings

test:
	cargo test --workspace $(CARGO_FLAGS)

test-all:
	cargo test --workspace --all-features $(CARGO_FLAGS)

test-no-default:
	cargo test --workspace $(NO_DEFAULT) $(CARGO_FLAGS)

doc:
	RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features $(CARGO_FLAGS)

security:
	cargo deny check

build:
	cargo build --workspace $(CARGO_FLAGS)

publish-dry:
	cargo publish --dry-run --allow-dirty

release release-snapshot:
	goreleaser release --clean $(if $(filter release-snapshot,$@),--snapshot,--skip=publish,validate)

clean:
	cargo clean
	rm -rf dist
