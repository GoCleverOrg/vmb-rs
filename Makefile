# ============================================================================
# vmb-rs workspace targets
#
# Mirrors the mira Makefile's dev-experience (fmt/clippy/deny/shear/taplo/
# typos, per-crate mutants, bench harness) without any Nix-specific env
# wiring — vmb-rs has no native deps beyond the optional Vimba X SDK.
# ============================================================================

.PHONY: clean clean-rust

clean-rust:
	@cargo clean

clean: clean-rust

# Set RUSTC_WRAPPER=sccache to enable compilation caching.
RUSTC_WRAPPER ?=

RUST_ENV := \
	$(if $(RUSTC_WRAPPER),RUSTC_WRAPPER=$(RUSTC_WRAPPER),)

# --- Quality checks ---

.PHONY: rust-fmt rust-clippy rust-deny rust-shear rust-typos rust-taplo rust-lint

rust-fmt:
	@cargo fmt --check

rust-clippy:
	@$(RUST_ENV) cargo clippy --workspace --all-targets -- -D warnings

rust-deny:
	@cargo deny check

rust-shear:
	@cargo shear

rust-typos:
	@typos

rust-taplo:
	@taplo check

rust-lint: rust-fmt rust-clippy rust-taplo rust-typos rust-deny rust-shear
	@echo "All lints passed."

# --- Testing ---

.PHONY: rust-test rust-nextest rust-test-features

rust-test:
	@$(RUST_ENV) cargo test --workspace

rust-nextest:
	@$(RUST_ENV) cargo nextest run --workspace

rust-test-features:
	@$(RUST_ENV) cargo hack check --feature-powerset --workspace

# --- Mutation testing ---

# -j1 is fastest: incremental compilation cache is per-build-dir, and
# parallel jobs each get a fresh temp dir losing all cache benefit.
MUTANTS_JOBS    ?= 1
MUTANTS_TIMEOUT ?= 120

# Per-crate output layout: $(MUTANTS_OUT)/<crate>/mutants.out/{caught,missed,...}.txt
# Lives under target/ so it's already gitignored.
MUTANTS_OUT := target/mutants
# Only `vmb-core` is under the mutation-coverage gate. The other four
# crates are coverage-exempt by design:
#   - vmb-sys:  bindgen-generated; bindings.rs already excluded via
#               .cargo/mutants.toml.
#   - vmb-ffi:  pure FFI adapter; killing its mutants requires hardware
#               acceptance tests on a self-hosted runner with the
#               Vimba X SDK installed.
#   - vmb-fake: test infrastructure; implicitly verified by every
#               assertion in vmb-core/tests/.
#   - vmb:     facade re-exports + a one-line `real()` constructor.
# To force a one-off sweep over a different crate, use:
#   make rust-mutants-crate CRATE=vmb-fake
CRATES      := vmb-core

# Warm-path optimizations from the mira mutants-perf experiments.
# See docs/mutants-perf-learnings.md in the mira repo for the 60-experiment
# rationale. Ported knobs:
#
# --baseline=skip: skip the baseline un-mutated test run. PRECONDITION:
#   callers must run `make test` (or equivalent) first. Intentionally NOT
#   wired as a Makefile dependency so CI pipelines and local users can
#   control ordering externally.
# --profile=mutants-bench: scoped profile defined in Cargo.toml.
MUTANTS_FLAGS := --baseline=skip --profile=mutants-bench

# Suppress cargo progress-bar rendering (measurable per-frame cost on
# small warm builds).
MUTANTS_TERM_ENV := \
	CARGO_TERM_QUIET=true \
	CARGO_TERM_PROGRESS_WHEN=never \
	NO_COLOR=1

.PHONY: rust-mutants rust-mutants-fast rust-mutants-crate rust-mutants-file rust-mutants-bench rust-mutants-report

rust-mutants:
	@mkdir -p $(MUTANTS_OUT)
	@FAIL=0; \
	for crate in $(CRATES); do \
		echo "=== mutants: $$crate ==="; \
		$(MUTANTS_TERM_ENV) $(RUST_ENV) cargo mutants --package $$crate \
			--timeout=$(MUTANTS_TIMEOUT) --in-place \
			--output $(MUTANTS_OUT)/$$crate \
			$(MUTANTS_FLAGS) || FAIL=1; \
	done; \
	$(MAKE) rust-mutants-report; \
	exit $$FAIL

rust-mutants-fast:
	@mkdir -p $(MUTANTS_OUT)
	@DIFF=$$(mktemp) && git diff origin/main > $$DIFF && \
	FAIL=0; \
	for crate in $(CRATES); do \
		echo "=== mutants (diff): $$crate ==="; \
		$(MUTANTS_TERM_ENV) $(RUST_ENV) cargo mutants --package $$crate \
			--timeout=$(MUTANTS_TIMEOUT) --in-place \
			--in-diff $$DIFF \
			--output $(MUTANTS_OUT)/$$crate \
			$(MUTANTS_FLAGS) || FAIL=1; \
	done; \
	rm -f $$DIFF; \
	$(MAKE) rust-mutants-report; \
	exit $$FAIL

rust-mutants-crate:
	@test -n "$(CRATE)" || (echo "Usage: make rust-mutants-crate CRATE=vmb" && exit 1)
	@mkdir -p $(MUTANTS_OUT)
	@$(MUTANTS_TERM_ENV) $(RUST_ENV) cargo mutants --package $(CRATE) \
		--timeout=$(MUTANTS_TIMEOUT) --in-place \
		--output $(MUTANTS_OUT)/$(CRATE) \
		$(MUTANTS_FLAGS)

rust-mutants-file:
	@test -n "$(FILE)" || (echo "Usage: make rust-mutants-file FILE=vmb/src/error.rs" && exit 1)
	@$(MUTANTS_TERM_ENV) $(RUST_ENV) cargo mutants \
		--timeout=$(MUTANTS_TIMEOUT) --in-place \
		--file $(FILE) \
		$(MUTANTS_FLAGS)

rust-mutants-bench:
	@echo "=== Mutation Test Benchmark (per crate) ==="
	@mkdir -p $(MUTANTS_OUT)
	@LABEL="$${BENCH_LABEL:-default}"; \
	for crate in $(CRATES); do \
		echo "--- $$crate ---"; \
		START=$$(date +%s); \
		$(MUTANTS_TERM_ENV) $(RUST_ENV) cargo mutants --package $$crate \
			--timeout=$(MUTANTS_TIMEOUT) --in-place \
			--output $(MUTANTS_OUT)/$$crate \
			$(MUTANTS_FLAGS) || true; \
		END=$$(date +%s); \
		ELAPSED=$$((END - START)); \
		DIR=$(MUTANTS_OUT)/$$crate/mutants.out; \
		C=$$(wc -l < $$DIR/caught.txt 2>/dev/null | tr -d ' '); \
		M=$$(wc -l < $$DIR/missed.txt 2>/dev/null | tr -d ' '); \
		U=$$(wc -l < $$DIR/unviable.txt 2>/dev/null | tr -d ' '); \
		T=$$(wc -l < $$DIR/timeout.txt 2>/dev/null | tr -d ' '); \
		TESTABLE=$$((C + M)); \
		if [ $$TESTABLE -gt 0 ]; then \
			SCORE=$$(echo "scale=1; $$C * 100 / $$TESTABLE" | bc); \
		else \
			SCORE="0.0"; \
		fi; \
		echo "$$(date -Iseconds) | $$LABEL | $$crate | score=$$SCORE% | caught=$$C missed=$$M unviable=$$U timeout=$$T | $${ELAPSED}s | -j$(MUTANTS_JOBS)" \
			>> mutants-bench.log; \
	done; \
	echo "Appended per-crate rows to mutants-bench.log"
	@$(MAKE) rust-mutants-report

rust-mutants-report:
	@printf "%-28s %8s %8s %8s %8s %8s\n" CRATE CAUGHT MISSED UNVIA TIMEOUT SCORE
	@printf '%.0s-' $$(seq 1 74); echo
	@TOTAL_C=0; TOTAL_M=0; TOTAL_U=0; TOTAL_T=0; \
	for crate in $(CRATES); do \
		DIR=$(MUTANTS_OUT)/$$crate/mutants.out; \
		if [ -d $$DIR ]; then \
			C=$$(wc -l < $$DIR/caught.txt 2>/dev/null | tr -d ' '); \
			M=$$(wc -l < $$DIR/missed.txt 2>/dev/null | tr -d ' '); \
			U=$$(wc -l < $$DIR/unviable.txt 2>/dev/null | tr -d ' '); \
			T=$$(wc -l < $$DIR/timeout.txt 2>/dev/null | tr -d ' '); \
			TESTABLE=$$((C + M)); \
			if [ $$TESTABLE -gt 0 ]; then \
				S=$$(echo "scale=1; $$C * 100 / $$TESTABLE" | bc)%; \
			else \
				S="-"; \
			fi; \
			printf "%-28s %8d %8d %8d %8d %8s\n" $$crate $$C $$M $$U $$T $$S; \
			TOTAL_C=$$((TOTAL_C + C)); \
			TOTAL_M=$$((TOTAL_M + M)); \
			TOTAL_U=$$((TOTAL_U + U)); \
			TOTAL_T=$$((TOTAL_T + T)); \
		else \
			printf "%-28s %8s %8s %8s %8s %8s\n" $$crate - - - - -; \
		fi; \
	done; \
	printf '%.0s-' $$(seq 1 74); echo; \
	TESTABLE=$$((TOTAL_C + TOTAL_M)); \
	if [ $$TESTABLE -gt 0 ]; then \
		S=$$(echo "scale=1; $$TOTAL_C * 100 / $$TESTABLE" | bc)%; \
	else \
		S="-"; \
	fi; \
	printf "%-28s %8d %8d %8d %8d %8s\n" TOTAL $$TOTAL_C $$TOTAL_M $$TOTAL_U $$TOTAL_T $$S

# --- Convenience aliases ---

.PHONY: test lint mutants

test: rust-test
lint: rust-lint
mutants: rust-mutants
