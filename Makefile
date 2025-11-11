.PHONY: help all clean build release test test-postgres test-sqlite lint fmt check-fmt markdownlint nixie corpus sqlite postgres sqlite-release postgres-release

APP ?= mxd
CARGO ?= cargo
BUILD_JOBS ?=
CLIPPY_FLAGS ?= --workspace --all-targets --all-features -- -D warnings
MDLINT ?= markdownlint-cli2
NIXIE ?= nixie
RSTEST_TIMEOUT ?= 20
SQLITE_FEATURES := --features sqlite
POSTGRES_FEATURES := --no-default-features --features postgres
TEST_SQLITE_FEATURES := --features "sqlite test-support"
TEST_POSTGRES_FEATURES := --no-default-features --features "postgres test-support"
POSTGRES_TARGET_DIR := target/postgres

all: release ## Build release binaries for sqlite and postgres

help: ## Show available targets
	@grep -E '^[a-zA-Z_-]+:.*?##' $(MAKEFILE_LIST) | \
	awk 'BEGIN {printf "Available targets:\n"} match($$0, /^([a-zA-Z_-]+):[^#]*##[ 	]*(.*)$$/, m) {printf "  %-20s %s\n", m[1], m[2]}'

build: sqlite postgres ## Build debug binaries for sqlite and postgres

release: sqlite-release postgres-release ## Build release binaries for sqlite and postgres

clean: ## Remove build artefacts
	$(CARGO) clean
	rm -rf $(POSTGRES_TARGET_DIR)

corpus: ## Generate the fuzzing corpus
	$(CARGO) run --bin gen_corpus

fmt: ## Format Rust and Markdown sources
	$(CARGO) fmt --all
	mdformat-all

check-fmt: ## Verify formatting for Rust sources
	$(CARGO) fmt --all -- --check

lint: ## Run Clippy with warnings denied
	$(CARGO) clippy $(CLIPPY_FLAGS)

markdownlint: ## Lint Markdown files
	$(MDLINT) '**/*.md'

nixie: ## Validate Mermaid diagrams
	$(NIXIE) --no-sandbox

test: test-postgres test-sqlite ## Run sqlite and postgres test suites

test-postgres: ## Run tests with the postgres backend
	RSTEST_TIMEOUT=$(RSTEST_TIMEOUT) RUSTFLAGS="-D warnings" $(CARGO) test $(TEST_POSTGRES_FEATURES) -- --nocapture

test-sqlite: ## Run tests with the sqlite backend
	RSTEST_TIMEOUT=$(RSTEST_TIMEOUT) RUSTFLAGS="-D warnings" $(CARGO) test $(TEST_SQLITE_FEATURES)

sqlite: target/debug/$(APP) ## Build debug sqlite binary

postgres: $(POSTGRES_TARGET_DIR)/debug/$(APP) ## Build debug postgres binary

sqlite-release: target/release/$(APP) ## Build release sqlite binary

postgres-release: $(POSTGRES_TARGET_DIR)/release/$(APP) ## Build release postgres binary

target/debug/$(APP):
	$(CARGO) build $(BUILD_JOBS) --bin $(APP) $(SQLITE_FEATURES)

$(POSTGRES_TARGET_DIR)/debug/$(APP):
	$(CARGO) build $(BUILD_JOBS) --bin $(APP) $(POSTGRES_FEATURES) --target-dir $(POSTGRES_TARGET_DIR)

target/release/$(APP):
	$(CARGO) build $(BUILD_JOBS) --release --bin $(APP) $(SQLITE_FEATURES)

$(POSTGRES_TARGET_DIR)/release/$(APP):
	$(CARGO) build $(BUILD_JOBS) --release --bin $(APP) $(POSTGRES_FEATURES) --target-dir $(POSTGRES_TARGET_DIR)
