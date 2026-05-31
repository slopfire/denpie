set dotenv-load := true

setup:
  sh scripts/bootstrap-dev.sh

backend:
  DENPIE_SKIP_FRONTEND_BUILD=1 cargo run

frontend:
  cd frontend && env -u NO_COLOR trunk watch

dev:
  sh scripts/dev.sh

check:
  DENPIE_SKIP_FRONTEND_BUILD=1 cargo check --workspace

test:
  DENPIE_SKIP_FRONTEND_BUILD=1 cargo test --workspace

fmt:
  cargo fmt --all

lint:
  DENPIE_SKIP_FRONTEND_BUILD=1 cargo clippy --workspace --all-targets -- -D warnings

frontend-build:
  cd frontend && env -u NO_COLOR trunk build --release

ci:
  cargo fmt --all --check
  DENPIE_SKIP_FRONTEND_BUILD=1 cargo clippy --workspace --all-targets -- -D warnings
  DENPIE_SKIP_FRONTEND_BUILD=1 cargo test --workspace
  cd frontend && env -u NO_COLOR trunk build --release

bench:
  sh benches/run_bench.sh

clean-dev:
  rm -rf frontend/dist frontend/.trunk frontend/.dev-build-stamp
