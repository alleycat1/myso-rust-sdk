# Set the default target of this Makefile
.PHONY: all
all:: ci

.PHONY: check-features
check-features:
	$(MAKE) -C crates/-sdk-types check-features
	$(MAKE) -C crates/-crypto check-features

.PHONY: check-fmt
check-fmt:
	cargo fmt -- --config imports_granularity=Item --check

.PHONY: fmt
fmt:
	cargo fmt -- --config imports_granularity=Item

.PHONY: clippy
clippy:
	cargo clippy --all-features --all-targets

.PHONY: test
test:
	cargo nextest run --all-features -p -sdk-types -p -crypto
	cargo test --doc

package_%.json: crates/-transaction-builder/tests/%/Move.toml crates/-transaction-builder/tests/%/sources/*.move
	cd crates/-transaction-builder/tests/$(*F) &&  move build --ignore-chain --dump-bytecode-as-base64 > ../../$@

.PHONY: test-with-localnet
test-with-localnet: package_test_example_v1.json package_test_example_v2.json
	cargo nextest run -p -graphql-client -p -transaction-builder

.PHONY: wasm
wasm:
	$(MAKE) -C crates/-sdk-types wasm
	$(MAKE) -C crates/-crypto wasm

.PHONY: doc
doc:
	RUSTDOCFLAGS="--cfg=doc_cfg -Zunstable-options --generate-link-to-definition" RUSTC_BOOTSTRAP=1 cargo doc --all-features --no-deps

.PHONY: doc-open
doc-open:
	RUSTDOCFLAGS="--cfg=doc_cfg -Zunstable-options --generate-link-to-definition" RUSTC_BOOTSTRAP=1 cargo doc --all-features --no-deps --open

.PHONY: ci
ci: check-features check-fmt test wasm

.PHONY: ci-full
ci-full: ci doc

.PHONY: clean
clean:
	cargo clean

.PHONY: clean-all
clean-all: clean
	git clean -dX
