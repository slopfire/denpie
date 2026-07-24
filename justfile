set dotenv-load := true
set shell := ["sh", "-cu"]

# Enter a Nix shell with the pinned Rust toolchain, Trunk, protoc, and build deps.
shell:
  nix-shell

setup:
  sh scripts/bootstrap-dev.sh

backend:
  DENPIE_SKIP_FRONTEND_BUILD=1 cargo run

frontend:
  cd frontend && env -u NO_COLOR trunk watch

dev:
  sh scripts/dev.sh

# --- verification tiers -------------------------------------------------------

# Fastest loop while editing: fmt check + compile (no tests, no frontend rebuild).
quick:
  cargo fmt --all --check
  DENPIE_SKIP_FRONTEND_BUILD=1 cargo check --workspace

# Alias kept for agents/docs that still say `just check`.
check: quick

# Run one filtered test target, e.g. `just test-one grounding` or `just test-one test_login`.
test-one filter:
  DENPIE_SKIP_FRONTEND_BUILD=1 cargo test --workspace {{filter}}

# Full unit/integration suite (no frontend rebuild).
test:
  DENPIE_SKIP_FRONTEND_BUILD=1 cargo test --workspace

fmt:
  cargo fmt --all

lint:
  DENPIE_SKIP_FRONTEND_BUILD=1 cargo clippy --workspace --all-targets -- -D warnings

# One full local gate: fmt + clippy + tests. Prefer this once at the end of a task.
verify:
  cargo fmt --all --check
  DENPIE_SKIP_FRONTEND_BUILD=1 cargo clippy --workspace --all-targets -- -D warnings
  DENPIE_SKIP_FRONTEND_BUILD=1 cargo test --workspace

frontend-build:
  cd frontend && env -u NO_COLOR trunk build --release

# Frontend release build + isolated agent-server oneshot smoke on :3027.
ui-check:
  cd frontend && env -u NO_COLOR trunk build --release
  sh scripts/agent-server.sh --oneshot --keep-data

# Full gate including release frontend build (CI-shaped).
ci:
  cargo fmt --all --check
  DENPIE_SKIP_FRONTEND_BUILD=1 cargo clippy --workspace --all-targets -- -D warnings
  DENPIE_SKIP_FRONTEND_BUILD=1 cargo test --workspace
  cd frontend && env -u NO_COLOR trunk build --release

# --- agent runtime ------------------------------------------------------------

# Isolated server on :3027 only. Creates .agent-data/, test login, smoke checks.
# Pass-through args: --stop, --smoke, --keep-data
agent-server *args:
  sh scripts/agent-server.sh {{args}}

bench:
  sh benches/run_bench.sh

clean-dev:
  rm -rf frontend/dist frontend/.trunk frontend/.dev-build-stamp .agent-data
