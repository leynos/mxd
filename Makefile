.PHONY: help all clean build release test test-doc test-postgres test-sqlite test-wireframe-only test-verification validator-sqlite-server validator-postgres-server test-validator-sqlite test-validator-postgres lint lint-postgres lint-sqlite lint-wireframe-only typecheck typecheck-postgres typecheck-sqlite typecheck-wireframe-only fmt check-fmt markdownlint nixie audit rust-audit corpus sqlite postgres sqlite-release postgres-release tlc tlc-handshake spelling spelling-config spelling-config-write spelling-phrase-check spelling-helper-test

export PATH := $(HOME)/.cargo/bin:$(HOME)/.local/bin:$(HOME)/.bun/bin:$(PATH)

APP ?= mxd
CARGO ?= cargo
CARGO_FALLBACK := $(HOME)/.cargo/bin/cargo
CARGO_CMD := $(firstword $(CARGO))
CARGO_PATH := $(shell command -v $(CARGO_CMD) 2>/dev/null)
ifeq ($(CARGO_PATH),)
  ifneq ($(wildcard $(CARGO_FALLBACK)),)
    CARGO := $(CARGO_FALLBACK)
    CARGO_CMD := $(firstword $(CARGO))
    CARGO_PATH := $(shell command -v $(CARGO_CMD) 2>/dev/null)
  endif
endif
CARGO_BIN_DIR := $(if $(CARGO_PATH),$(dir $(CARGO_PATH)))
LOCAL_BIN_DIR := $(HOME)/.local/bin
BUILD_JOBS ?=
# Prefer cargo-nextest when installed, per the estate convention in
# agent-template-rust's template/Makefile.jinja; fall back to cargo test.
TEST_CMD := $(if $(shell $(CARGO) nextest --version 2>/dev/null),nextest run,test)
CLIPPY_FLAGS ?= --workspace --all-targets -- -D warnings
WHITAKER ?= whitaker
WHITAKER_FALLBACK := $(HOME)/.local/bin/whitaker
WHITAKER_CMD := $(firstword $(WHITAKER))
WHITAKER_PATH := $(shell command -v $(WHITAKER_CMD) 2>/dev/null)
ifeq ($(WHITAKER_PATH),)
  ifneq ($(wildcard $(WHITAKER_FALLBACK)),)
    WHITAKER := $(WHITAKER_FALLBACK)
    WHITAKER_CMD := $(firstword $(WHITAKER))
    WHITAKER_PATH := $(shell command -v $(WHITAKER_CMD) 2>/dev/null)
  endif
endif
MDLINT ?= markdownlint-cli2
MDLINT_FALLBACK := $(HOME)/.bun/bin/markdownlint-cli2
ifneq ($(wildcard $(MDLINT_FALLBACK)),)
  ifneq ($(shell command -v $(MDLINT) >/dev/null 2>&1; echo $$?),0)
    MDLINT := $(MDLINT_FALLBACK)
  endif
endif
WHITAKER_BIN_DIR := $(if $(WHITAKER_PATH),$(dir $(WHITAKER_PATH)))
TOOL_PATH_PREFIX := $(shell printf '%s\n' "$(CARGO_BIN_DIR)" "$(WHITAKER_BIN_DIR)" "$(LOCAL_BIN_DIR)" | awk 'NF { printf "%s%s", sep, $$0; sep=":" }')
NIXIE ?= nixie
UV ?= uv
UV_ENV = UV_CACHE_DIR=.uv-cache UV_TOOL_DIR=.uv-tools
RUFF_VERSION ?= 0.15.12
PATHSPEC_VERSION ?= 1.1.1
TYPOS_VERSION ?= 1.48.0
TYPOS_CONFIG_BUILDER_COMMIT := d6da92f02240a79a945c835f69bdd08a888da1d0
TYPOS_CONFIG_BUILDER_SOURCE := git+https://github.com/leynos/typos-config-builder.git@$(TYPOS_CONFIG_BUILDER_COMMIT)
TYPOS_CONFIG_BUILDER := $(UV_ENV) $(UV) tool run --python 3.14 \
	--from "$(TYPOS_CONFIG_BUILDER_SOURCE)" typos-config-builder
SPELLING_PY_SRCS := \
	scripts/typos_rollout_check.py scripts/tests/test_typos_rollout_check.py
SPELLING_PY_TESTS := scripts/tests/test_typos_rollout_check.py
SPELLING_COVERAGE_ARGS := --cov=typos_rollout_check --cov-fail-under=90
SPELLING_HELPER_PYTEST = PYTHONPATH=scripts $(UV_ENV) $(UV) run --no-project \
	--python 3.14 --with pathspec==$(PATHSPEC_VERSION) --with pytest==9.0.2 \
	--with pytest-cov==7.0.0 python -m pytest
TLC_RUNNER ?= ./scripts/run-tlc.sh
TLC_IMAGE ?= ghcr.io/leynos/mxd/mxd-tlc:latest
RSTEST_TIMEOUT ?= 20
SQLITE_FEATURES := --features sqlite
POSTGRES_FEATURES := --no-default-features --features "postgres legacy-networking"
TEST_SQLITE_FEATURES := --features "sqlite test-support"
TEST_POSTGRES_FEATURES := --no-default-features --features "postgres test-support legacy-networking"
WIREFRAME_ONLY_FEATURES := --no-default-features --features "sqlite toml test-support"
POSTGRES_TARGET_DIR := target/postgres

all: check-fmt typecheck lint test spelling

help: ## Show available targets
	@grep -E '^[a-zA-Z_-]+:.*?##' $(MAKEFILE_LIST) | \
	awk 'BEGIN {printf "Available targets:\n"} match($$0, /^([a-zA-Z_-]+):[^#]*##[ 	]*(.*)$$/, m) {printf "  %-20s %s\n", m[1], m[2]}'

build: sqlite postgres ## Build debug binaries for sqlite and postgres

release: sqlite-release postgres-release ## Build release binaries for sqlite and postgres

clean: ## Remove build artefacts
	$(CARGO) clean
	rm -rf $(POSTGRES_TARGET_DIR)
	rm -rf .uv-cache .uv-tools

corpus: ## Generate the fuzzing corpus
	$(CARGO) run --bin gen_corpus

fmt: ## Format Rust and Markdown sources
	$(CARGO) fmt --all
	mdformat-all

check-fmt: ## Verify formatting for Rust sources
	$(CARGO) fmt --all -- --check

typecheck: typecheck-postgres typecheck-sqlite typecheck-wireframe-only ## Run cargo check for all feature sets

typecheck-postgres: ## Run cargo check with the postgres backend
	$(CARGO) check $(TEST_POSTGRES_FEATURES)

typecheck-sqlite: ## Run cargo check with the sqlite backend
	$(CARGO) check $(TEST_SQLITE_FEATURES)

typecheck-wireframe-only: ## Run cargo check with legacy networking disabled
	$(CARGO) check $(WIREFRAME_ONLY_FEATURES)

lint: lint-postgres lint-sqlite lint-wireframe-only ## Run Clippy and Whitaker for all feature sets

lint-postgres: ## Run Clippy and Whitaker with the postgres backend
	$(CARGO) clippy $(TEST_POSTGRES_FEATURES) $(CLIPPY_FLAGS)
	PATH="$(TOOL_PATH_PREFIX)$(if $(TOOL_PATH_PREFIX),:)$$PATH" RUSTFLAGS="-D warnings" $(WHITAKER) --all -- $(TEST_POSTGRES_FEATURES) --all-targets

lint-sqlite: ## Run Clippy and Whitaker with the sqlite backend
	$(CARGO) clippy $(TEST_SQLITE_FEATURES) $(CLIPPY_FLAGS)
	PATH="$(TOOL_PATH_PREFIX)$(if $(TOOL_PATH_PREFIX),:)$$PATH" RUSTFLAGS="-D warnings" $(WHITAKER) --all -- $(TEST_SQLITE_FEATURES) --all-targets

lint-wireframe-only: ## Run Clippy and Whitaker with legacy networking disabled
	$(CARGO) clippy $(WIREFRAME_ONLY_FEATURES) $(CLIPPY_FLAGS)
	PATH="$(TOOL_PATH_PREFIX)$(if $(TOOL_PATH_PREFIX),:)$$PATH" RUSTFLAGS="-D warnings" $(WHITAKER) --all -- $(WIREFRAME_ONLY_FEATURES) --all-targets

markdownlint: spelling ## Lint Markdown files and enforce spelling
	$(MDLINT) "**/*.md" "#.uv-cache" "#.uv-tools"

spelling: spelling-phrase-check ## Enforce en-GB-oxendict in tracked text
	@git ls-files -z | xargs -0 -r env $(UV_ENV) \
		$(UV) tool run typos@$(TYPOS_VERSION) --config typos.toml --force-exclude --hidden

spelling-phrase-check: spelling-config ## Reject prohibited spelling phrases
	@PYTHONPATH=scripts $(UV_ENV) $(UV) run --no-project --python 3.14 scripts/typos_rollout_check.py --repository .

spelling-config: spelling-helper-test ## Verify generated spelling configuration
	@git ls-files --error-unmatch typos.toml >/dev/null
	@$(TYPOS_CONFIG_BUILDER) --repository . --check

spelling-config-write: spelling-helper-test ## Generate spelling configuration
	@$(TYPOS_CONFIG_BUILDER) --repository .

spelling-helper-test: ## Validate the shared spelling-policy integration
	@$(UV_ENV) $(UV) tool run ruff@$(RUFF_VERSION) format --isolated --target-version py313 --check $(SPELLING_PY_SRCS)
	@$(UV_ENV) $(UV) tool run ruff@$(RUFF_VERSION) check --isolated --target-version py313 $(SPELLING_PY_SRCS)
	@$(SPELLING_HELPER_PYTEST) $(SPELLING_PY_TESTS) -c /dev/null --rootdir=. -p no:cacheprovider $(SPELLING_COVERAGE_ARGS)

nixie: ## Validate Mermaid diagrams
	$(NIXIE) --no-sandbox

audit: rust-audit ## Audit dependencies for known vulnerabilities

rust-audit: ## Audit every Rust manifest for known vulnerabilities
	audited_file=$$(mktemp); \
	skipped_file=$$(mktemp); \
	trap 'rm -f "$$audited_file" "$$skipped_file"' EXIT; \
	find . \
		\( -path '*/target/*' -o -path '*/node_modules/*' -o -path '*/.venv/*' \) -prune -o \
		-name Cargo.toml -exec sh -c 'set -e; audited_file=$$1; skipped_file=$$2; shift; shift; for manifest do \
			manifest_dir=$$(dirname "$$manifest"); \
			if [ ! -f "$$manifest_dir/Cargo.lock" ]; then \
				printf "Skipping Rust manifest without adjacent lockfile %s\n" "$$manifest"; \
				printf . >> "$$skipped_file"; \
				continue; \
			fi; \
			printf "Auditing Rust manifest %s\n" "$$manifest"; \
			if (cd "$$manifest_dir" && $(CARGO) audit); then \
				printf . >> "$$audited_file"; \
			else \
				rc=$$?; \
				printf "VULNERABILITY FAILURE: cargo audit failed for %s (exit %d)\n" "$$manifest" $$rc; \
				exit $$rc; \
			fi; \
		done' sh "$$audited_file" "$$skipped_file" {} +; \
	printf "Audit summary: $$(wc -c < $$audited_file) manifest(s) audited, $$(wc -c < $$skipped_file) manifest(s) skipped (no adjacent Cargo.lock)\n"; \
	if [ ! -s "$$audited_file" ]; then \
		printf "No lockfile-backed Rust manifests were audited\n"; \
		exit 1; \
	fi

tlc: tlc-handshake ## Run all TLA+ model checks

tlc-handshake: ## Run TLC on handshake spec
	TLC_IMAGE=$(TLC_IMAGE) $(TLC_RUNNER) crates/mxd-verification/tla/MxdHandshake.tla

test: test-postgres test-sqlite test-wireframe-only test-verification test-doc ## Run sqlite, postgres, wireframe-only, verification, and doc suites

# Note: RSTEST_TIMEOUT is intentionally omitted for postgres tests because
# TestCluster is !Send (uses ScopedEnv with PhantomData<*const ()>) and rstest's
# timeout feature requires Send. See docs/pg-embed-setup-unpriv-users-guide.md
# "Thread safety constraints (v0.4.0)" for details.
test-postgres: ## Run tests with the postgres backend
	RUSTFLAGS="-D warnings" $(CARGO) $(TEST_CMD) $(TEST_POSTGRES_FEATURES)

test-sqlite: ## Run tests with the sqlite backend
	RSTEST_TIMEOUT=$(RSTEST_TIMEOUT) RUSTFLAGS="-D warnings" $(CARGO) $(TEST_CMD) $(TEST_SQLITE_FEATURES)

test-wireframe-only: ## Run tests with legacy networking disabled
	RSTEST_TIMEOUT=$(RSTEST_TIMEOUT) RUSTFLAGS="-D warnings" $(CARGO) $(TEST_CMD) $(WIREFRAME_ONLY_FEATURES)

test-verification: ## Run verification crate tests
	RUSTFLAGS="-D warnings" $(CARGO) $(TEST_CMD) -p mxd-verification

# nextest does not execute doctests; run them separately with the
# default (sqlite) backend, mirroring the template's split.
test-doc: ## Run documentation tests
	RUSTFLAGS="-D warnings" $(CARGO) test --doc $(TEST_SQLITE_FEATURES)

validator-sqlite-server: ## Build the sqlite wireframe server binary for validator runs
	$(MAKE) APP=mxd-wireframe-server sqlite

validator-postgres-server: ## Build the postgres wireframe server binary for validator runs
	$(MAKE) APP=mxd-wireframe-server postgres

test-validator-sqlite: validator-sqlite-server ## Run the hx validator against the sqlite wireframe server
	MXD_VALIDATOR_SERVER_BINARY=$(CURDIR)/target/debug/mxd-wireframe-server \
		RUSTFLAGS="-D warnings" $(CARGO) test -p validator --features sqlite

test-validator-postgres: validator-postgres-server ## Run the hx validator against the postgres wireframe server
	MXD_VALIDATOR_SERVER_BINARY=$(CURDIR)/$(POSTGRES_TARGET_DIR)/debug/mxd-wireframe-server \
		RUSTFLAGS="-D warnings" $(CARGO) test -p validator --no-default-features --features postgres

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
