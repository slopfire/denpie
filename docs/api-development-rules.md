# API development rules

These are build rules, not suggestions. `just api-check` runs locally as part of
`just quick`, `just verify`, and `just ci`; the same gate runs in CI.

## V1 compatibility boundary

`POST /api/v1`, `proto/denpie.proto`, and `api/operations-v1.json` form one
contract. Existing v1 messages, fields, enum values, reservations, operations,
result fields, authentication modes, scopes, and mutation classifications are
immutable. A change to any of them requires a new API major such as `/api/v2`.

Allowed v1 changes are additive:

- a new field with a never-used field number;
- a new enum value with a never-used number;
- a new request/response message;
- a new operation and success-result oneof field;
- documentation, examples, or comments that do not alter wire semantics.

Do not remove a v1 member, rename it, change its type/number/presence/oneof,
reuse a reservation, weaken authentication, change its scope, reclassify a read
as a mutation (or the reverse), or change its successful result variant.

## New operation checklist

1. Add request/response messages and oneof fields to `proto/denpie.proto`.
2. Add the operation metadata and exact `ApiResponse.result` field to
   `api/operations-v1.json`.
3. Classify it in `mutation_policy` and `required_scope` in
   `src/api/transport.rs`.
4. Keep the transport thin; shared behavior belongs in `src/services/` and SQL
   belongs in a repository with bound parameters.
5. Mutations must use `Replayable` unless their successful response contains a
   one-time secret. V1 automatically rejects mutation requests without an
   idempotency key and routes them through the durable idempotency store.
6. Return the registered success oneof field. Build-generated Rust code checks
   the mapping exhaustively, and runtime validation converts a mismatch into a
   structured internal error instead of publishing an undocumented response.
7. Add integration coverage for authentication, scope denial, success, the
   main validation/not-found failure, and idempotency/replay for mutations.
8. Update `docs/api-v1.md`, examples, and `docs/api-changelog.md` in the same
   change.
9. Run `just api-contract-update`, inspect and commit the additive ledger diff,
   then run the normal verification gate.

## What the gate enforces

`just api-check` fails when:

- a protected protobuf message/field/enum/reservation is removed or changed;
- a compatible addition was not explicitly recorded in the v1 ledger;
- an operation is missing from the manifest or Rust policy matches;
- request/result types or exact success oneof fields disagree with the schema;
- auth, scope, read/mutation, idempotency, or result policy drifts;
- the generated operation reference is stale;
- the contract checker's own breaking/additive regression tests fail.

`cargo check` adds another boundary: `build.rs` generates exhaustive operation
and result matches from the manifest. Adding a protobuf operation/result without
registering it cannot compile. Every successful `/api` and `/api/v1` response is
validated against that generated mapping at runtime.

The ledger update command is deliberately monotonic. It accepts additions and
refuses breaking changes. CI additionally compares it with the pull request's
base revision (and local checks compare with `HEAD`), so removing a member from
both the schema and ledger still fails. Do not hand-edit
`api/contract-v1.json` to bypass it; start a new major contract instead.
