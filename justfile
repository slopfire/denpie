set dotenv-load := true
set shell := ["sh", "-cu"]

# Enter a Nix shell with the pinned Rust toolchain, Trunk, protoc, and build deps.
shell:
  nix-shell

setup:
  sh scripts/bootstrap-dev.sh

db-up:
  docker compose -f compose.dev.yaml up -d --wait postgres

db-down:
  docker compose -f compose.dev.yaml down

# Explicitly destructive: removes the local PostgreSQL volume.
db-reset:
  docker compose -f compose.dev.yaml down --volumes
  docker compose -f compose.dev.yaml up -d --wait postgres

backend:
  DENPIE_SKIP_FRONTEND_BUILD=1 cargo run --bin denpie

frontend:
  cd frontend && env -u NO_COLOR trunk watch

dev:
  sh scripts/dev.sh

# Opt-in research runner. Never part of just test / verify / ci.
# libcaesium alone is optimized in the shared dev profile so lab builds reuse it.
lab *args:
  DENPIE_SKIP_FRONTEND_BUILD=1 cargo run --bin denpie-lab -- {{args}}

# Render checked-in card fixtures with the production Yew FlowCard on :3027.
lab-cards-ui:
  cd frontend && env -u NO_COLOR trunk serve --features lab-ui --port 3027

# Deterministic offline proof for the lab CLI, fixtures, comparisons, and lab UI build.
lab-check:
  sh scripts/check-lab.sh

# --- verification tiers -------------------------------------------------------

# Fastest loop while editing: fmt check + compile (no tests, no frontend rebuild).
quick: api-check
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

# Regenerate the complete operation table after protobuf or policy changes.
api-reference:
  python3 scripts/generate-api-reference.py

# Fast, dependency-light guard for wire compatibility and operation policy drift.
api-check:
  sh scripts/check-api-contract.sh

# Record additive v1 members. Breaking changes fail and require a new API major.
api-contract-update:
  python3 scripts/generate-api-reference.py
  python3 scripts/check-api-contract.py --update
  sh scripts/check-api-contract.sh

# Package the canonical proto, descriptor set, manifest, and checksums.
api-schema:
  sh scripts/package-api-schema.sh

# Check generated API docs and execute offline curl/Python/TypeScript/Rust examples.
docs-check:
  sh scripts/check-api-docs.sh

# One full local gate: fmt + clippy + tests. Prefer this once at the end of a task.
verify: api-check
  cargo fmt --all --check
  DENPIE_SKIP_FRONTEND_BUILD=1 cargo clippy --workspace --all-targets -- -D warnings
  DENPIE_SKIP_FRONTEND_BUILD=1 cargo test --workspace

frontend-build:
  cd frontend && env -u NO_COLOR trunk build --release

# Frontend release build + isolated agent-server oneshot smoke on :3027.
ui-check:
  cd frontend && env -u NO_COLOR trunk build --release
  sh scripts/agent-server.sh --oneshot --keep-data

# Browser UI verification through the isolated :3027 agent server.
playwright:
  bun run test:ui

# Install the repo-local Playwright runner and its Chromium binary.
playwright-install:
  bun install
  PLAYWRIGHT_BROWSERS_PATH=0 bunx playwright install chromium

# Full gate including release frontend build (CI-shaped).
ci: api-check
  cargo fmt --all --check
  DENPIE_SKIP_FRONTEND_BUILD=1 cargo clippy --workspace --all-targets -- -D warnings
  DENPIE_SKIP_FRONTEND_BUILD=1 cargo test --workspace
  sh scripts/check-api-docs.sh
  cd frontend && env -u NO_COLOR trunk build --release

# --- agent runtime ------------------------------------------------------------

# Isolated server on :3027 only. Creates .agent-data/, test login, smoke checks.
# Pass-through args: --stop, --smoke, --keep-data
agent-server *args:
  sh scripts/agent-server.sh {{args}}

bench:
  sh benches/run_bench.sh

clean-dev:
  rm -rf frontend/dist frontend/.trunk frontend/.dev-build-stamp .agent-data test-results playwright-report blob-report
