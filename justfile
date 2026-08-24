set dotenv-load := true
set shell := ["sh", "-cu"]

# Enter a Nix shell with the pinned Rust toolchain, bun, protoc, and build deps.
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
  cd frontend-astro && bun run dev

# Backend on :3017 + Astro/Vite on :4321 (API/session routes proxied to Axum).
dev:
  sh scripts/dev.sh

# Alias kept for docs that still say `just dev-astro`.
dev-astro: dev

# Opt-in research runner. Never part of just test / verify / ci.
# libcaesium alone is optimized in the shared dev profile so lab builds reuse it.
lab *args:
  DENPIE_SKIP_FRONTEND_BUILD=1 cargo run --bin denpie-lab -- {{args}}

# Render checked-in card fixtures with the production FlowCard on :3027.
lab-cards-ui:
  sh scripts/build-frontend.sh
  cd frontend-astro && ASTRO_PREVIEW_BACKGROUND=0 bun run preview -- --host 127.0.0.1 --port 3027

# Fast card-polish loop with Astro HMR. This is opt-in and never part of CI.
lab-cards-dev:
  cd frontend-astro && ASTRO_DEV_BACKGROUND=0 bun run dev -- --ignore-lock --host 127.0.0.1 --port 3027

# Opt-in light/dark and responsive screenshot matrix. Artifacts stay ignored.
lab-cards-shot output="test-results/lab-card-matrix":
  LAB_CARD_SCREENSHOT_DIR='{{output}}' PLAYWRIGHT_BROWSERS_PATH=0 bunx playwright test --config tests/e2e-astro/lab-cards.config.mjs --grep @screenshot

# Compare two local run directories in the static review workbench on :3027.
lab-review baseline candidate:
  DENPIE_LAB_REVIEW_BASELINE='{{baseline}}' DENPIE_LAB_REVIEW_CANDIDATE='{{candidate}}' sh scripts/build-frontend.sh
  cd frontend-astro && ASTRO_PREVIEW_BACKGROUND=0 bun run preview -- --host 127.0.0.1 --port 3027

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

# --- Astro frontend (frontend-astro/), see docs/frontend-astro.md ---

# Install Astro frontend dependencies.
frontend-astro-install:
  cd frontend-astro && bun install --frozen-lockfile

# Strict TypeScript check for the Astro frontend.
frontend-astro-typecheck:
  cd frontend-astro && bun run typecheck

# Static release build of the Astro frontend into frontend-astro/dist/.
frontend-astro-build:
  sh scripts/build-frontend.sh

frontend-build: frontend-astro-build

# Rebuild the offline topic-icon subset from config/topic_icons.json.
frontend-astro-topic-icons:
  cd frontend-astro && bun run generate:topic-icons

# Fail when the checked-in offline topic-icon subset has drifted.
frontend-astro-topic-icons-check:
  cd frontend-astro && bun run check:topic-icons

# Fail when visible Astro UI copy bypasses the translation catalog.
frontend-astro-i18n-check:
  cd frontend-astro && bun run check:i18n

# Focused Bun tests for the Astro frontend (offline, no server).
frontend-astro-test: frontend-astro-topic-icons-check frontend-astro-i18n-check
  cd frontend-astro && bun test

# Regenerate frontend-astro protobuf TypeScript from the canonical
# proto/denpie.proto (system protoc + project-local protoc-gen-es).
frontend-astro-protogen:
  cd frontend-astro && bun run generate:proto

# Fail if the checked-in generated output differs from a fresh regeneration.
frontend-astro-proto-check:
  sh scripts/check-proto-gen.sh

# Build the Astro frontend, then serve it through the isolated :3027 agent server
# (oneshot smoke).
frontend-astro-runtime:
  sh scripts/build-frontend.sh
  sh scripts/agent-server.sh --oneshot --keep-data

# Frontend release build + isolated agent-server oneshot smoke on :3027.
ui-check: frontend-astro-runtime

# Browser UI verification through the isolated :3027 agent server.
playwright:
  bunx playwright test --config tests/e2e-astro/playwright.config.mjs

playwright-astro: playwright

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
  cd frontend-astro && bun install --frozen-lockfile
  cd frontend-astro && bun run check:topic-icons
  cd frontend-astro && bun run check:i18n
  cd frontend-astro && bun test
  sh scripts/build-frontend.sh

# --- agent runtime ------------------------------------------------------------

# Isolated server on :3027 only. Creates .agent-data/, test login, smoke checks.
# Pass-through args: --stop, --smoke, --keep-data
agent-server *args:
  sh scripts/agent-server.sh {{args}}

bench:
  sh benches/run_bench.sh

clean-dev:
  rm -rf frontend-astro/dist frontend-astro/.astro frontend-astro/.dev-build-stamp .agent-data test-results playwright-report blob-report
