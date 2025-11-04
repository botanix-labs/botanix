
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
