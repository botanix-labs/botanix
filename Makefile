-include .env

SHELL := /usr/bin/env bash
.SHELLFLAGS := -o pipefail -e -c

# Heavily inspired by Lighthouse: https://github.com/sigp/lighthouse/blob/693886b94176faa4cb450f024696cb69cda2fe58/Makefile
.DEFAULT_GOAL := help

# Get the latest git tag, or use 'dev' as fallback if no tags exist
GIT_TAG ?= $(shell git describe --tags --abbrev=0 2>/dev/null || echo "dev")

# Features for builds (empty = use default features)
FEATURES ?=

# Conditional features flag - only add --features if FEATURES is not empty
FEATURES_FLAG := $(if $(FEATURES),--features "$(FEATURES)",)

# Cargo profile for builds. Default is for local builds, CI uses an override.
PROFILE ?= release

# Extra flags for Cargo
CARGO_INSTALL_EXTRA_FLAGS ?=

# The docker image name
DOCKER_IMAGE_NAME ?= ghcr.io/botanix-labs/$(BUILD_PACKAGE)

# Botanix local network configuration
NODES_DIR ?= .botanix-local
# Resolve NODES_DIR to absolute path, expanding ~ properly
NODES_DIR_ABS := $(shell bash -c 'echo $(NODES_DIR)')

# Number of nodes to run in the Botanix local network
NODES_NUMBER ?= 2
# Number of min signers
FROST_MIN_SIGNERS ?= 2
# Number of max signers
FROST_MAX_SIGNERS ?= 2

# Package to build (reth or btc-server)
BUILD_PACKAGE ?= botanix-reth
# Binary to build (reth or btc-server)
BUILD_BIN ?= botanix-reth

# Build output directory (Cargo target directory)
BUILD_PATH ?= target
# Directory for staging binaries before Docker build
BIN_DIR ?= bin

##@ Help

.PHONY: help
help: ## Display this help.
	@awk 'BEGIN {FS = ":.*##"; printf "Usage:\n  make \033[36m<target>\033[0m\n"} /^[a-zA-Z_0-9-]+:.*?##/ { printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)

.PHONY: list-features
list-features: ## List available features for BUILD_PACKAGE (default: botanix-reth).
	@echo "Available features for $(BUILD_PACKAGE):"
	@cargo metadata --format-version 1 --no-deps 2>/dev/null | \
		jq -r '.packages[] | select(.name == "$(BUILD_PACKAGE)") | .features | keys[]' 2>/dev/null || \
		{ echo "Note: Install 'jq' for better output, or check Cargo.toml directly:"; \
		  echo "  grep -A 50 '^\[features\]' bin/$(BUILD_PACKAGE)/Cargo.toml"; }

##@ Build

.PHONY: install
install: ## Build and install the reth binary under `~/.cargo/bin`.
	cargo install --path bin/botanix-reth --bin botanix-reth --force --locked \
		$(FEATURES_FLAG) \
		--profile "$(PROFILE)" \
		$(CARGO_INSTALL_EXTRA_FLAGS)

.PHONY: install-btc-server
install-btc-server: ## Build and install the btc-server binary under `~/.cargo/bin`.
	cargo install --path bin/botanix-btc-server --bin botanix-btc-server --force --locked \
		$(FEATURES_FLAG) \
		--profile "$(PROFILE)" \
		$(CARGO_INSTALL_EXTRA_FLAGS)

.PHONY: build
build: ## Build the reth binary into `target` directory.
	cargo build --bin botanix-reth $(FEATURES_FLAG) --profile "$(PROFILE)"

.PHONY: build-debug
build-debug: ## Build the reth binary into `target/debug` directory.
	cargo build --bin botanix-reth $(FEATURES_FLAG)
# Builds the reth binary natively.
build-native-%:
	cargo build --bin botanix-reth --target $* $(FEATURES_FLAG) --profile "$(PROFILE)"

# The following commands use `cross` to build a cross-compile.
#
# These commands require that:
#
# - `cross` is installed (`cargo install cross`).
# - Docker is running.
# - The current user is in the `docker` group.
#
# The resulting binaries will be created in the `target/` directory.

# For aarch64, set the page size for jemalloc.
# When cross compiling, we must compile jemalloc with a large page size,
# otherwise it will use the current system's page size which may not work
# on other systems. JEMALLOC_SYS_WITH_LG_PAGE=16 tells jemalloc to use 64-KiB
# pages. See: https://github.com/paradigmxyz/reth/issues/6742
build-aarch64-unknown-linux-gnu: export JEMALLOC_SYS_WITH_LG_PAGE=16

# No jemalloc on Windows
build-x86_64-pc-windows-gnu: FEATURES := $(filter-out jemalloc jemalloc-prof,$(FEATURES))

# Note: The additional rustc compiler flags are for intrinsics needed by MDBX.
# See: https://github.com/cross-rs/cross/wiki/FAQ#undefined-reference-with-build-std
build-%:
	RUSTFLAGS="-C link-arg=-lgcc -Clink-arg=-static-libgcc" \
		cross build --package $(BUILD_PACKAGE) --bin $(BUILD_BIN) --target $* $(FEATURES_FLAG) --profile "$(PROFILE)"


build-btc-server-%:
	cross build --package botanix-btc-server --bin botanix-btc-server --target $* --release

##@ Test

UNIT_TEST_ARGS := --locked --workspace --features 'jemalloc-prof' -E 'kind(lib)' -E 'kind(bin)' -E 'kind(proc-macro)'
UNIT_TEST_ARGS_OP := --locked --workspace --features 'jemalloc-prof' -E 'kind(lib)' -E 'kind(bin)' -E 'kind(proc-macro)'
COV_FILE := lcov.info

.PHONY: test-unit
test-unit: ## Run unit tests.
	cargo install cargo-nextest --locked
	cargo nextest run $(UNIT_TEST_ARGS)

.PHONY: test-unit-op
test-unit-op: ## Run unit tests
	cargo install cargo-nextest --locked
	cargo nextest run $(UNIT_TEST_ARGS_OP)

.PHONY: cov-unit
cov-unit: ## Run unit tests with coverage.
	rm -f $(COV_FILE)
	cargo llvm-cov nextest --lcov --output-path $(COV_FILE) $(UNIT_TEST_ARGS)

.PHONY: cov-unit-op
cov-unit-op: ## Run unit tests with coverage
	rm -f $(COV_FILE)
	cargo llvm-cov nextest --lcov --output-path $(COV_FILE) $(UNIT_TEST_ARGS_OP)

.PHONY: cov-report-html
cov-report-html: cov-unit ## Generate a HTML coverage report and open it in the browser.
	cargo llvm-cov report --html
	open target/llvm-cov/html/index.html

##@ Docker

# Note: This requires a buildx builder with emulation support. For example:
#
# `docker run --privileged --rm tonistiigi/binfmt --install amd64,arm64`
# `docker buildx create --use --driver docker-container --name cross-builder`
.PHONY: docker-build-push
docker-build-push: ## Build and push a cross-arch Docker image tagged with the latest git tag.
	$(call docker_build_push,$(GIT_TAG),$(GIT_TAG))

# Note: This requires a buildx builder with emulation support. For example:
#
# `docker run --privileged --rm tonistiigi/binfmt --install amd64,arm64`
# `docker buildx create --use --driver docker-container --name cross-builder`
.PHONY: docker-build-push-latest
docker-build-push-latest: ## Build and push a cross-arch Docker image tagged with the latest git tag and `latest`.
	$(call docker_build_push,$(GIT_TAG),latest)

# Note: This requires a buildx builder with emulation support. For example:
#
# `docker run --privileged --rm tonistiigi/binfmt --install amd64,arm64`
# `docker buildx create --use --name cross-builder`
.PHONY: docker-build-push-nightly
docker-build-push-nightly: ## Build and push cross-arch Docker image tagged with the latest git tag with a `-nightly` suffix, and `latest-nightly`.
	$(call docker_build_push,$(GIT_TAG)-nightly,latest-nightly)

# Create a cross-arch Docker image with the given tags and push it
define docker_build_push
	@if [ -z "$(1)" ] || [ -z "$(2)" ]; then \
		echo "Error: Docker tags cannot be empty. GIT_TAG=$(GIT_TAG)"; \
		echo "Either create a git tag or set GIT_TAG manually: make docker-build-push GIT_TAG=v1.0.0"; \
		exit 1; \
	fi

	docker buildx build --file ./Dockerfile . \
		--platform linux/amd64,linux/arm64 \
		--tag $(DOCKER_IMAGE_NAME):$(1) \
		--tag $(DOCKER_IMAGE_NAME):$(2) \
		--provenance=false \
		--build-arg PACKAGE=$(BUILD_PACKAGE) \
        --build-arg BIN=$(BUILD_BIN) \
        --build-arg PROFILE=$(PROFILE) \
		--push
endef

##@ Other

.PHONY: clean
clean: ## Perform a `cargo` clean and remove the binary and test vectors directories.
	cargo clean
	rm -rf $(BIN_DIR)

# ------------------------------------------------------------
#  Setup & Validation Targets
# ------------------------------------------------------------

install:
	cargo fetch

validate-env: check-commands check-versions check-dev-env
	@echo "Validating Rust toolchain..."
	@rustc --version | grep -q "$(shell cat rust-toolchain 2>/dev/null || echo "$(RUST_VERSION)")" || { echo "Wrong rustc version"; exit 1; }
	@echo "Validating cargo installation..."
	@cargo --version >/dev/null 2>&1 || { echo "cargo is required but not installed"; exit 1; }
	@echo "Environment validation complete"

check-commands:
	@for cmd in rustup npm pre-commit docker python3; do \
		if ! command -v $$cmd >/dev/null 2>&1; then \
			echo "$$cmd is not installed. Please install $$cmd and try again."; \
			exit 1; \
		fi \
	done

check-versions:
	@echo "Checking required tool versions..."
	@echo "$$(rustc --version)"
	@echo "$$(cargo --version)"
	@echo "node: $$(node --version)"
	@echo "npm: $$(npm --version)"

check-dev-env:
	@if [ ! -f .env ]; then \
		echo "Warning: .env file not found. Copying from .env.example..."; \
		cp .env.example .env; \
	fi

# ------------------------------------------------------------
#  Development Targets
# ------------------------------------------------------------

dev-watch:
	cargo watch -- cargo run

clean: clean-build

clean-build:
	cargo clean
	rm -rf target/
	rm -rf node_modules/

# ------------------------------------------------------------
#  Formatting & Linting
# ------------------------------------------------------------

# Convert find output to space-separated list for taplo
TOML_FILES := $(shell find . -not -path "./target/*" -name "*.toml" | tr '\n' ' ')
BUN := $(shell command -v bun 2>/dev/null || echo "${HOME}/.bun/bin/bun")

fmt: fmt-cargo fmt-rust fmt-prettier fmt-markdown

fmt-cargo:
	@echo "Formatting TOML files..."
	@taplo fmt $(TOML_FILES)

fmt-rust:
	cargo fmt -- --color always

fmt-prettier:
	@$(BUN) run prettier:fix

fmt-markdown:
	@$(BUN) run md:fix

lint: lint-cargo lint-rust lint-clippy lint-prettier lint-markdown lint-machete

lint-cargo:
	@taplo fmt --check $(TOML_FILES)

lint-rust:
	@cargo check --all-targets --all-features
	@cargo fmt --all --check -- --color always

lint-clippy:
	cargo clippy --workspace --all-targets --all-features -- -D warnings

lint-prettier:
	bun run prettier:validate

lint-markdown:
	bun run md:lint

lint-machete:
	cargo machete --skip-target-dir

# ------------------------------------------------------------
#  Audit
# ------------------------------------------------------------

audit:
	cargo audit

audit-fix-test:
	cargo audit fix --dry-run

audit-fix:
	cargo audit fix

# ------------------------------------------------------------
#  Build & Documentation
# ------------------------------------------------------------

build:
	cargo build --release

docs: doc
	@echo "Generating additional documentation..."
	@cargo doc --no-deps --document-private-items
	@cargo doc --workspace --no-deps

docs-serve: docs
	@echo "Serving documentation on http://localhost:8000"
	@python3 -m http.server 8000 --directory target/doc


# ------------------------------------------------------------
# Documentation
# ------------------------------------------------------------

.PHONY: rustdocs
rustdocs: ## Runs `cargo docs` to generate the Rust documents in the `target/doc` directory
	RUSTDOCFLAGS="\
	--cfg docsrs \
	--show-type-layout \
	--generate-link-to-definition \
	--enable-index-page -Zunstable-options -D warnings" \
	cargo +nightly docs \
	--document-private-items

# ------------------------------------------------------------
# Tests
# ------------------------------------------------------------

test-reth:
	cargo test \
	--workspace \
	--bin "botanix-reth" \
	--lib \
	--tests \
	--benches \
	--features "all"

test-other-targets:
	cargo test \
	--workspace \
	--lib \
	--tests \
	--benches \
	--all-features

test-doc:
	cargo test --doc --workspace --features "all"

test:
	make test-reth && \
	make test-doc && \
	make test-other-targets

pr:
	make lint && \
	make update-book-cli && \
	make test

# ------------------------------------------------------------
# Audit
# ------------------------------------------------------------

audit:
	cargo audit

audit-fix-test:
	cargo audit fix --dry-run

audit-fix:
	cargo audit fix

# ------------------------------------------------------------
# Coverage
# ------------------------------------------------------------

coverage:
	RUSTFLAGS="-Z threads=8" cargo +nightly tarpaulin --config ./tarpaulin.toml

clean-unused-deps:
	cargo machete --fix

# ------------------------------------------------------------
# Botanix
# ------------------------------------------------------------

start-test-suite-runners:
	cd ./bin/botanix-test-suite && \
	/usr/local/bin/botanix-test-suite \
	--test-to-run "${TEST_TO_RUN}" \
	--config "./config.toml" \
	--run-suite all \
	--timeout 500000 \
	--dry-run false \
	--min-signers 3 \
	--max-signers 4 \
	--rpc-nodes 1 \
	--syncing-nodes 1

start-test-suite:
	cd ./bin/botanix-test-suite && \
	cargo run --bin botanix-test-suite -- \
	--test-to-run "${TEST_TO_RUN}" \
	--config "./config.toml" \
	--run-suite all \
	--timeout 500000 \
	--dry-run false \
	--min-signers 3 \
	--max-signers 4 \
	--rpc-nodes 1 \
	--syncing-nodes 1

start-test-suite-build:
	cargo build -p botanix-btc-server --bin botanix-btc-server && \
	cargo build -p botanix-reth --bin botanix-reth && \
	cd ./bin/botanix-test-suite && \
	cargo run --bin botanix-test-suite -- \
	--test-to-run "${TEST_TO_RUN}" \
	--config "./config.toml" \
	--run-suite all \
	--timeout 500000 \
	--dry-run false \
	--min-signers 3 \
	--max-signers 4 \
	--rpc-nodes 1 \
	--syncing-nodes 1

start-btc-server-1:
	cd ./bin/botanix-btc-server && \
	cargo run --bin botanix-btc-server -- \
	--identifier 0 \
	--coordinator 0 \
	--federation-config-path "${NODE_1_DIR}/federation.toml" \
	--p2p-secret-key "${NODE_1_DIR}/discovery-secret" \
	--address 0.0.0.0:8081 \
	--db "./db1" \
	--min-signers 2 \
	--max-signers 2 \
	--toml ./config.toml \
	--fee-rate-diff-percentage 30 \
	--btc-network "${BITCOIND_NETWORK}" \
	--bitcoind-url "${BITCOIND_URL}" \
	--bitcoind-user "${BITCOIND_USER}" \
	--bitcoind-pass "${BITCOIND_PWD}" \
	--btc-signing-server-jwt-secret "${NODE_1_DIR}/bjwt.hex" \
	--fall-back-fee-rate-sat-per-vbyte 5

start-btc-server-2:
	cd ./bin/botanix-btc-server && \
	cargo run --bin botanix-btc-server -- \
	--identifier 1 \
	--coordinator 0 \
	--federation-config-path "${NODE_2_DIR}/federation.toml" \
	--p2p-secret-key "${NODE_2_DIR}/discovery-secret" \
	--address 0.0.0.0:8082 \
	--db "./db2" \
	--min-signers 2 \
	--max-signers 2 \
	--toml ./config.toml \
	--fee-rate-diff-percentage 30 \
	--btc-network "${BITCOIND_NETWORK}" \
	--bitcoind-url "${BITCOIND_URL}" \
	--bitcoind-user "${BITCOIND_USER}" \
	--bitcoind-pass "${BITCOIND_PWD}" \
	--btc-signing-server-jwt-secret "${NODE_2_DIR}/bjwt.hex" \
	--fall-back-fee-rate-sat-per-vbyte 5

start-btc-server-3:
	cd ./bin/botanix-btc-server && \
	cargo run --bin botanix-btc-server -- \
	--identifier 2 \
	--coordinator 0 \
	--federation-config-path "${NODE_3_DIR}/federation.toml" \
	--p2p-secret-key "${NODE_3_DIR}/discovery-secret" \
	--address 0.0.0.0:8083 \
	--db "./db3" \
	--min-signers 3 \
	--max-signers 3 \
	--toml ./config.toml \
	--fee-rate-diff-percentage 30 \
	--btc-network "${BITCOIND_NETWORK}" \
	--bitcoind-url "${BITCOIND_URL}" \
	--bitcoind-user "${BITCOIND_USER}" \
	--bitcoind-pass "${BITCOIND_PWD}" \
	--btc-signing-server-jwt-secret "${NODE_3_DIR}/bjwt.hex" \
	--fall-back-fee-rate-sat-per-vbyte 5

start-poa-server-1:
	cd ./bin/botanix-reth && \
	cargo run --bin botanix-reth -- node \
	--chain=botanix-testnet \
	--is-testnet \
	--federation-config-path "${NODE_1_DIR}/federation.toml" \
	--federation-mode \
	--datadir ${NODE_1_DIR} \
	--metrics "127.0.0.1:9001" \
	--http \
	--http.corsdomain "*" \
	--http.port 8545 \
	--http.addr "127.0.0.1" \
	--http.api eth,net,trace,txpool,web3,rpc,admin \
	--ws \
	--ws.origins "*" \
	--ws.port 9545 \
	--ws.addr "127.0.0.1" \
	--ws.api eth,net,trace,txpool,web3,rpc,admin \
	-vvv \
	--btc-server "localhost:8081" \
	--btc-network "${BITCOIND_NETWORK}" \
	--btc-signing-server-jwt-secret-path "${NODE_1_DIR}/bjwt.hex" \
	--bitcoind.primary_url "${BITCOIND_URL}" \
	--bitcoind.primary_username "${BITCOIND_USER}" \
	--bitcoind.primary_password "${BITCOIND_PWD}" \
	--frost.min_signers 2 \
	--frost.max_signers 2 \
	--sync.num_snapshots_to_keep 3 \
	--p2p-secret-key "${NODE_1_DIR}/discovery-secret" \
	--port 30303 \
	--abci-port=26658 \
	--sync.enable_state_sync \
	--sync.enable_historical_sync \
	--block-fee-recipient-address "${BLOCK_FEE_RECIPIENT_ADDRESS}" \
	--txpool.minimum-priority-fee 2500000 \
	--txpool.minimal-protocol-fee 5000000 \
	--cometbft-rpc-port=26657

start-poa-server-2:
	cd ./bin/botanix-reth && \
	cargo run --bin botanix-reth -- node \
	--chain=botanix-testnet \
	--is-testnet \
	--federation-config-path "${NODE_2_DIR}/federation.toml" \
	--federation-mode \
	--datadir ${NODE_2_DIR} \
	--metrics "127.0.0.1:9002" \
	--http \
	--http.corsdomain "*" \
	--http.port 8546 \
	--http.addr "127.0.0.1" \
	--http.api eth,net,trace,txpool,web3,rpc,admin \
	--ws \
	--ws.origins "*" \
	--ws.port 9546 \
	--ws.addr "127.0.0.1" \
	--ws.api eth,net,trace,txpool,web3,rpc,admin \
	-vvv \
	--btc-server "localhost:8082" \
	--btc-network "${BITCOIND_NETWORK}" \
	--btc-signing-server-jwt-secret-path "${NODE_2_DIR}/bjwt.hex" \
	--bitcoind.primary_url "${BITCOIND_URL}" \
	--bitcoind.primary_username "${BITCOIND_USER}" \
	--bitcoind.primary_password "${BITCOIND_PWD}" \
	--frost.min_signers 2 \
	--frost.max_signers 2 \
	--sync.num_snapshots_to_keep 3 \
	--p2p-secret-key "${NODE_2_DIR}/discovery-secret" \
	--port 30304 \
	--abci-port=36658 \
	--sync.enable_state_sync \
	--sync.enable_historical_sync \
	--block-fee-recipient-address "${BLOCK_FEE_RECIPIENT_ADDRESS}" \
	--txpool.minimum-priority-fee 2500000 \
	--txpool.minimal-protocol-fee 5000000 \
	--cometbft-rpc-port=36657

start-poa-server-3:
	cd ./bin/botanix-reth && \
	cargo run --bin botanix-reth -- node \
	--chain=botanix-testnet \
	--is-testnet \
	--federation-config-path "${NODE_3_DIR}/federation.toml" \
	--federation-mode \
	--datadir ${NODE_3_DIR} \
	--metrics "127.0.0.1:9003" \
	--http \
	--http.corsdomain "*" \
	--http.port 8547 \
	--http.addr "127.0.0.1" \
	--http.api eth,net,trace,txpool,web3,rpc,admin \
	--ws \
	--ws.origins "*" \
	--ws.port 9547 \
	--ws.addr "127.0.0.1" \
	--ws.api eth,net,trace,txpool,web3,rpc,admin \
	-vvv \
	--btc-server "localhost:8083" \
	--btc-network "${BITCOIND_NETWORK}" \
	--btc-signing-server-jwt-secret-path "${NODE_3_DIR}/bjwt.hex" \
	--bitcoind.primary_url "${BITCOIND_URL}" \
	--bitcoind.primary_username "${BITCOIND_USER}" \
	--bitcoind.primary_password "${BITCOIND_PWD}" \
	--frost.min_signers 3 \
	--frost.max_signers 3 \
	--sync.num_snapshots_to_keep 3 \
	--p2p-secret-key "${NODE_3_DIR}/discovery-secret" \
	--port 30305 \
	--abci-port=46658 \
    --sync.enable_state_sync \
	--sync.enable_historical_sync \
	--block-fee-recipient-address "${BLOCK_FEE_RECIPIENT_ADDRESS}" \
	--txpool.minimum-priority-fee 2500000 \
	--txpool.minimal-protocol-fee 5000000 \
	--cometbft-rpc-port=46657

start-non-fed-server-1:
	cd ./bin/botanix-reth && \
	cargo run --bin botanix-reth -- node \
	--chain=botanix-testnet \
	--is-testnet \
	--federation-config-path "${NON_FED_1_DIR}/federation.toml" \
	--datadir ${NON_FED_1_DIR} \
	--http \
	--http.corsdomain "*" \
	--http.port 8548 \
	--http.addr "127.0.0.1" \
	--http.api eth,net,trace,txpool,web3,rpc,admin \
	--ws \
	--ws.origins "*" \
	--ws.port 9548 \
	--ws.addr "127.0.0.1" \
	--ws.api eth,net,trace,txpool,web3,rpc,admin \
	-vvv \
	--btc-network "${BITCOIND_NETWORK}" \
	--bitcoind.primary_url "${BITCOIND_URL}" \
	--bitcoind.primary_username "${BITCOIND_USER}" \
	--bitcoind.primary_password "${BITCOIND_PWD}" \
	--p2p-secret-key "${NON_FED_1_DIR}/discovery-secret" \
	--port 30306 \
	--abci-port=56658 \
	--sync.enable_state_sync \
	--sync.enable_historical_sync \
  --txpool.minimum-priority-fee 2500000 \
  --txpool.minimal-protocol-fee 5000000

start-cometbft-1:
	cometbft node \
	--home "${NODE_1_DIR}/cometbft" \
	--proxy_app 127.0.0.1:26658 \
	--p2p.laddr tcp://0.0.0.0:26656 \
	--moniker node-1 \
	--rpc.laddr=tcp://0.0.0.0:26657 \
	--p2p.persistent_peers ${PERSISTENT_PEERS} \

start-cometbft-2:
	cometbft node \
	--home "${NODE_2_DIR}/cometbft" \
	--proxy_app 127.0.0.1:36658 \
	--p2p.laddr tcp://0.0.0.0:36656 \
	--moniker node-2 \
	--rpc.laddr=tcp://0.0.0.0:36657 \
	--p2p.persistent_peers ${PERSISTENT_PEERS} \

start-cometbft-3:
	cometbft node \
	--home "${NODE_3_DIR}/cometbft" \
	--proxy_app 127.0.0.1:46658 \
	--p2p.laddr tcp://0.0.0.0:46656 \
	--moniker node-3 \
	--rpc.laddr=tcp://0.0.0.0:46657 \
	--p2p.persistent_peers ${PERSISTENT_PEERS} \

clean-poa-3:
	cd ${NODE_3_DIR} && \
	rm -rf "${NODE_3_DIR}/db" && \
	rm -rf "${NODE_3_DIR}/botanix_db" && \
	rm -rf "${NODE_3_DIR}/static_files"

clean-poa-2:
	cd ${NODE_2_DIR} && \
	rm -rf "${NODE_2_DIR}/db" && \
	rm -rf "${NODE_2_DIR}/botanix_db" && \
	rm -rf "${NODE_2_DIR}/static_files"

clean-poa-1:
	cd ${NODE_1_DIR} && \
	rm -rf "${NODE_1_DIR}/db" && \
	rm -rf "${NODE_1_DIR}/botanix_db" && \
	rm -rf "${NODE_1_DIR}/static_files"

clean-rpc:
	cd ${NON_FED_1_DIR} && \
	rm -rf "${NON_FED_1_DIR}/db" && \
	rm -rf "${NON_FED_1_DIR}/botanix_db" && \
	rm -rf "${NON_FED_1_DIR}/static_files"


clean-btc-server-1:
	cd bin/botanix-btc-server && \
	rm -rf "db1"

clean-btc-server-2:
	cd bin/botanix-btc-server && \
	rm -rf "db2"

clean-btc-server-3:
	cd bin/botanix-btc-server && \
	rm -rf "db3"

make clean-all:
	make clean-btc-server-1
	make clean-btc-server-2
	make clean-btc-server-3
	make clean-poa-1
	make clean-poa-2
	make clean-poa-3
	make clean-rpc

check-features:
	cargo hack check \
		--package reth-codecs \
		--package reth-primitives-traits \
		--package reth-primitives \
		--package reth-rpc-types \
		--feature-powerset

.PHONY: bitcoin-cli
bitcoin-cli:
	@if [ -z "$(CMD)" ]; then \
		echo "Usage: make bitcoin-cli CMD='<command>'"; \
		echo "Example: make bitcoin-cli CMD='getblockchaininfo'"; \
		exit 0; \
	fi; \
	docker compose -f docker-local/docker-compose.bitcoin.yml exec bitcoin-core bitcoin-cli $(CMD);

.PHONY: init-docker-local
init-docker-local:
	# Generate network configs
	cargo run -p botanix-up -- \
		--num-nodes=${NODES_NUMBER} \
		--output-path=${NODES_DIR} \
		--multisig-min-signers=${FROST_MIN_SIGNERS} \
		--multisig-max-signers=${FROST_MAX_SIGNERS} \
		--docker-subnet=172.22.0.1/16

	# Create shared docker network
	docker network create \
      --subnet=172.22.0.0/16 \
      --ip-range=172.22.0.0/24 \
      --gateway=172.22.0.1 \
      botanix-local

	make init-bitcoin-core

.PHONY: init-bitcoin-core
init-bitcoin-core:
	# Start single bitcoin-core node
	docker compose --file docker-local/docker-compose.bitcoin.yml up -d

	# Create a wallet
	# https://developer.bitcoin.org/reference/rpc/createwallet.html
	# createwallet "wallet_name" ( disable_private_keys blank "passphrase" avoid_reuse descriptors load_on_startup )
	make bitcoin-cli CMD='--rpcwait createwallet local false false "" false false true';

	# Generate 10 blocks
	make bitcoin-cli CMD="-generate 10";

	# Stop the bitcoin-core node
	docker compose --file docker-local/docker-compose.bitcoin.yml stop

.PHONY: start-docker-local
start-docker-local:
	# Start single bitcoin-core node
	docker compose --file docker-local/docker-compose.bitcoin.yml up -d

	# Start nodes defined in the NODES_DIR
	make build-docker-local

.PHONY: restart-docker-local
restart-docker-local:
	# Restart single bitcoin-core node
	docker compose --file docker-local/docker-compose.bitcoin.yml restart

	# Restart nodes defined in the NODES_DIR
	@if [ ! -d "$(NODES_DIR_ABS)" ]; then \
		echo "Error: Nodes directory does not exist: $(NODES_DIR)"; \
		exit 1; \
	fi; \
	for DIR in $(NODES_DIR_ABS)/*/; do \
		if [ ! -f "$$DIR.env" ]; then \
			echo "Error: Environment file does not exist: $$DIR.env"; \
			exit 1; \
		fi; \
        docker compose \
        --env-file=.env \
        --env-file "$$DIR.env" \
        -f docker-local/docker-compose.yml \
        restart; \
	done

.PHONY: stop-docker-local
stop-docker-local:
	# Stop the bitcoin-core node
	docker compose --file docker-local/docker-compose.bitcoin.yml stop

	@if [ ! -d "$(NODES_DIR_ABS)" ]; then \
		echo "Error: Nodes directory does not exist: $(NODES_DIR)"; \
		exit 1; \
	fi; \
	for DIR in $(NODES_DIR_ABS)/*/; do \
		if [ ! -f "$$DIR.env" ]; then \
			echo "Error: Environment file does not exist: $$DIR.env"; \
			exit 1; \
		fi; \
		docker compose \
		--env-file=.env \
		--env-file "$$DIR.env" \
		-f docker-local/docker-compose.yml \
		stop; \
	done

.PHONY: build-docker-local
build-docker-local:
	@if [ ! -d "$(NODES_DIR_ABS)" ]; then \
		echo "Error: Nodes directory does not exist: $(NODES_DIR)"; \
		exit 1; \
	fi; \
	COMPOSE_BAKE=true docker compose --env-file "${NODES_DIR_ABS}/node-1/.env" -f docker-local/docker-compose.yml build; \
	for DIR in $(NODES_DIR_ABS)/*/; do \
		if [ ! -f "$$DIR.env" ]; then \
			echo "Error: Environment file does not exist: $$DIR.env"; \
			exit 1; \
		fi; \
		docker compose \
		--env-file=.env \
		--env-file "$$DIR.env" \
		-f docker-local/docker-compose.yml \
		up -d; \
	done

.PHONY: reset-docker-local
reset-docker-local:
	docker compose -f docker-local/docker-compose.bitcoin.yml down -v

	# Down nodes defined in the NODES_DIR
	for DIR in $(NODES_DIR_ABS)/*/; do \
		if [ -f "$$DIR.env" ]; then \
			docker compose \
			--env-file=.env \
			--env-file $$DIR.env \
			-f docker-local/docker-compose.yml \
			down -v; \
		fi; \
		rm -rf $${DIR}cometbft/data/*.db; \
	done

	make init-bitcoin-core

.PHONY: clean-docker-local
clean-docker-local:
	# Drop bitcoin-core data
	docker compose -f docker-local/docker-compose.bitcoin.yml down -v

	# Down nodes defined in the NODES_DIR
	for DIR in $(NODES_DIR_ABS)/*/; do \
		if [ -f "$$DIR.env" ]; then \
			docker compose \
			--env-file=.env \
			--env-file $$DIR.env \
			-f docker-local/docker-compose.yml \
			down -v; \
		fi; \
		rm -rf $${DIR}cometbft/data/*.db; \
	done

	# Remove docker network
	docker network rm -f botanix-local

	# Remove NODES_DIR
	rm -rf ${NODES_DIR_ABS}

clean-test-suite:
	cd bin/test-suite && \
	rm *.txt

# ------------------------------------------------------------
# Btc Server Profiling
# ------------------------------------------------------------

PROFILE_BTC_SERVER_ARGS := \
	--identifier ${PROFILER_FROST_ID} \
	--address 0.0.0.0:${PROFILER_BTC_SERVER_PORT} \
	--db ${PROFILER_DB_DIR} \
	--min-signers ${PROFILER_FROST_MIN_SIGNERS} \
	--max-signers ${PROFILER_FROST_MAX_SIGNERS} \
	--toml ./bin/botanix-btc-server/config.toml \
	--fee-rate-diff-percentage 30 \
	--btc-network ${BITCOIND_NETWORK} \
	--bitcoind-url ${BITCOIND_URL} \
	--bitcoind-user ${BITCOIND_USER} \
	--bitcoind-pass ${BITCOIND_PWD} \
	--btc-signing-server-jwt-secret ${PROFILER_NODE_DIR}/bjwt.hex \
	--fall-back-fee-rate-sat-per-vbyte 5

profile-btc:
	cargo build --profile profiling --package botanix-btc-server && \
	samply record ./target/profiling/botanix-btc-server $(PROFILE_BTC_SERVER_ARGS)

# ------------------------------------------------------------
# Poa Server Profiling
# ------------------------------------------------------------

PROFILER_POA_SEVER_ARGS := \
	node \
	--is-testnet \
	--federation-config-path ${PROFILER_NODE_DIR}/federation.toml \
	--federation-mode \
	--datadir ${PROFILER_NODE_DIR} \
	--http \
	--http.corsdomain "*" \
	--http.port ${PROFILER_POA_HTTP_PORT} \
	--http.addr 127.0.0.1 \
	--http.api eth,net,trace,txpool,web3,rpc,admin \
	--ws \
	--ws.origins "*" \
	--ws.port ${PROFILER_POA_WS_PORT} \
	--ws.addr 127.0.0.1 \
	--ws.api eth,net,trace,txpool,web3,rpc,admin \
	-vvv \
	--btc-server localhost:${PROFILER_BTC_SERVER_PORT} \
	--btc-network ${BITCOIND_NETWORK} \
	--btc-signing-server-jwt-secret-path ${PROFILER_NODE_DIR}/bjwt.hex \
	--bitcoind.primary_url "${BITCOIND_URL}" \
	--bitcoind.primary_username "${BITCOIND_USER}" \
	--bitcoind.primary_password "${BITCOIND_PWD}" \
	--frost.min_signers ${PROFILER_FROST_MIN_SIGNERS} \
	--frost.max_signers ${PROFILER_FROST_MAX_SIGNERS} \
	--p2p-secret-key ${PROFILER_NODE_DIR}/discovery-secret \
	--port ${PROFILER_POA_RPC_PORT} \
	--abci-port=${PROFILER_COMMET_ABCI_PORT}

profile-poa:
	cargo build --profile profiling --bin reth && \
	samply record ./target/profiling/reth $(PROFILER_POA_SEVER_ARGS)
