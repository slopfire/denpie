# Task
Implement the `algorithms` lab bench (SM-2 replay vs a lab-only 1-day baseline). Mark it `ready`. Do not break images/prompts/cards. No new production Algorithm enum arm. Do not claim FSRS.

# Facts
workspace: /home/sfire/Projects/slopfire/denpie

`src/lab/mod.rs` has images/prompts/cards ready; `run algorithms` still exits 2. Test `run_algorithms_exits_two_not_implemented` must change.

Clock already exists:
```
crate::scheduling::calculate_next_review_at(&mut SchedulingState, grade, now) -> DateTime<Utc>
```
`SchedulingState::default()` is SM2.

Dead code: `sm2::calculate_next_review` (the Utc::now wrapper) is unused in non-test lib builds because `scheduling/mod.rs` `calculate_next_review` currently calls `_at(..., Utc::now())`. Fix that in this change: make `scheduling::calculate_next_review` call `algorithms::sm2::calculate_next_review` so the wrapper is live. Keep `_at` for replay.

# Do
1. Tiny scheduling fix above. No other scheduling behavior change.
2. `src/lab/algorithms.rs` + `mod algorithms;` Dispatch `run algorithms`.
3. `lab/cases/algorithms/synthetic.json` **object**:
```
{"id":"synthetic-pass-fail","daily_card_count":3,"cards":["a","b","c"],
 "events":[{"card":"a","grade":4,"at":"2025-01-01T10:00:00Z"}, ...]}
```
≥3 cards, ~12 events, ISO-8601 `at`. Must include first pass (interval 1), second pass (interval 6), a third pass, fail grade 2 (reps reset), then a pass. Apply events in time order.

4. `--dry-run`/`--offline`, `--cases`, `--algorithm` (default `sm2`; `all` runs both). Candidates:
   - `sm2`: production `calculate_next_review_at` on a fresh SchedulingState per card
   - `fixed_1d`: **only in src/lab/**. Pass or fail => interval 1, next=now+1 day. Not in src/scheduling/, not an enum arm.

5. Dry-run: print cards, event count, date range, candidates. No lab/runs.
6. Live (no network): `lab/runs/<utc>-algorithms/scorecard.md|json` and `timeline.json`.
   Scorecard columns: candidate, n_events, n_pass (grade>=3), n_fail, mean_final_interval, max_final_interval, fail_resets, max_due_on_any_day (max over calendar days of how many cards that have been reviewed at least once have next_review_at.date <= that day).
7. list_benches: algorithms `ready`. Update USAGE and docs/lab.md.
8. Tests no network:
   - pack loads
   - dry-run exit 0 mentions sm2
   - replay helper: two successful grade-4 reviews on one card => interval 6 under sm2
   - list shows four ready
   - existing image/prompt/card tests still pass

Do not commit. Do not run live images or live prompts.

# Acceptance
```
just lab list
just lab run algorithms --dry-run
just lab run algorithms
DENPIE_SKIP_FRONTEND_BUILD=1 cargo test --workspace lab -- --nocapture
just test-one test_sm2
just quick
```
list: all four ready. dry-run and live exit 0. live writes scorecard. lab + sm2 tests pass. just quick has **no** unused `calculate_next_review` warning.
