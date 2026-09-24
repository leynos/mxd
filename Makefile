.PHONY: help all clean build release test test-doc test-postgres test-sqlite test-wireframe-only test-verification validator-sqlite-server validator-postgres-server test-validator-sqlite test-validator-postgres lint lint-postgres lint-sqlite lint-wireframe-only typecheck typecheck-postgres typecheck-sqlite typecheck-wireframe-only fmt check-fmt markdownlint nixie audit rust-audit corpus sqlite postgres sqlite-release postgres-release tlc tlc-handshake spelling test-codescene-boundary test-spelling-gate test-workflow-contracts check-locked test-dependabot-policy

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
# `make fmt` and `make check-fmt` call mdtablefix directly. `--git` selects the
# Markdown files Git tracks and `--include-untracked` adds the untracked files
# Git does not ignore, so a new document is formatted before it is staged.
# Both modes need mdtablefix 0.6.0 or later; CI pins the version at the
# install-mdtablefix step.
MDTABLEFIX ?= mdtablefix
MDTABLEFIX_SELECT = --git --include-untracked
MDTABLEFIX_RULES = --wrap --renumber --breaks --ellipsis --fences
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
# The commit v0.1.1 points at, not the tag. A tag is a movable ref: the same
# commit of this repository would run different code if it moved, and this
# target downloads and executes that code.
RUFF_VERSION ?= 0.15.12
PYYAML_VERSION ?= 6.0.3
# This is v0.1.1.
TYPOS_CONFIG_BUILDER_COMMIT ?= b2bc36bee84fbe9b958ab64bd4581377ddd60c72
TYPOS_CONFIG_BUILDER = $(UV_ENV) $(UV) tool run --python 3.14 --from \
	"git+https://github.com/leynos/typos-config-builder.git@$(TYPOS_CONFIG_BUILDER_COMMIT)" \
	typos-config-builder
# The tree the gate reads. Overridden only by the gate's own test, which runs
# this very target against a fixture holding a prohibited phrase.
SPELLING_ROOT ?= .
WORKFLOW_CONTRACT_SRCS := $(wildcard tests/workflow_contracts/*.py)
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
	$(MDTABLEFIX) --in-place $(MDTABLEFIX_SELECT) $(MDTABLEFIX_RULES)
	@unset FORCE_COLOR; $(MDLINT) --fix "**/*.md"

check-fmt: ## Verify formatting for Rust and Markdown sources
	$(CARGO) fmt --all -- --check
	$(MDTABLEFIX) --check $(MDTABLEFIX_SELECT) $(MDTABLEFIX_RULES)

check-locked: ## Refuse a lockfile the manifest does not admit
	$(CARGO) metadata --locked --format-version 1 >/dev/null

test-workflow-contracts: ## Assert the CI workflows place and gate what they claim
	@$(UV_ENV) $(UV) tool run ruff@$(RUFF_VERSION) format --isolated --target-version py313 --check $(WORKFLOW_CONTRACT_SRCS)
	@$(UV_ENV) $(UV) tool run ruff@$(RUFF_VERSION) check --isolated --target-version py313 $(WORKFLOW_CONTRACT_SRCS)
	@PYTHONPATH=tests/workflow_contracts $(UV_ENV) $(UV) run --no-project \
		--python 3.14 --with pytest==9.0.2 --with pyyaml==$(PYYAML_VERSION) \
		python -m pytest tests/workflow_contracts -c /dev/null --rootdir=. \
		-p no:cacheprovider

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

spelling: ## Enforce en-GB-oxendict in tracked text
	$(TYPOS_CONFIG_BUILDER) gate --repository $(SPELLING_ROOT) --scope all

SPELLING_GATE_SRCS := $(wildcard tests/spelling_gate/*.py)

test-spelling-gate: ## Prove the spelling gate rejects a prohibited phrase
	@$(UV_ENV) $(UV) tool run ruff@$(RUFF_VERSION) format --isolated \
		--target-version py313 --check $(SPELLING_GATE_SRCS)
	@$(UV_ENV) $(UV) tool run ruff@$(RUFF_VERSION) check --isolated \
		--target-version py313 $(SPELLING_GATE_SRCS)
	@PYTHONPATH=tests/spelling_gate $(UV_ENV) $(UV) run --no-project \
		--python 3.14 --with pytest==9.0.2 \
		python -m pytest tests/spelling_gate -c /dev/null --rootdir=. \
		-p no:cacheprovider

CODESCENE_BOUNDARY_SRCS := $(wildcard tests/codescene_boundary/*.py)

test-codescene-boundary: ## Assert CodeScene coverage stays owned by main
	@$(UV_ENV) $(UV) tool run ruff@$(RUFF_VERSION) format --isolated \
		--target-version py313 --check $(CODESCENE_BOUNDARY_SRCS)
	@$(UV_ENV) $(UV) tool run ruff@$(RUFF_VERSION) check --isolated \
		--target-version py313 $(CODESCENE_BOUNDARY_SRCS)
	@$(UV_ENV) $(UV) run --no-project --python 3.14 \
		--with pytest==9.0.2 --with pyyaml==$(PYYAML_VERSION) \
		python -m pytest tests/codescene_boundary -c /dev/null --rootdir=. \

DEPENDABOT_POLICY_SRCS := $(wildcard tests/dependabot_policy/*.py)

test-dependabot-policy: ## Assert the Dependabot configuration holds what it must
	@$(UV_ENV) $(UV) tool run ruff@$(RUFF_VERSION) format --isolated \
		--target-version py313 --check $(DEPENDABOT_POLICY_SRCS)
	@$(UV_ENV) $(UV) tool run ruff@$(RUFF_VERSION) check --isolated \
		--target-version py313 $(DEPENDABOT_POLICY_SRCS)
	@$(UV_ENV) $(UV) run --no-project --python 3.14 \
		--with pytest==9.0.2 --with pyyaml==$(PYYAML_VERSION) \
		python -m pytest tests/dependabot_policy -c /dev/null --rootdir=. \
		-p no:cacheprovider

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
