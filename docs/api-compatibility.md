# API compatibility and deprecation policy

This policy applies to the public protobuf endpoints and the checked-in
`denpie.proto` schema.

## API v1

`POST /api/v1` is the recommended stable integration surface. Within v1,
Denpie may make additive protobuf changes:

- add request or response fields with new field numbers;
- add operations to `ApiRequest.op` and results to `ApiResponse.result`;
- add enum values, capabilities, scopes, and optional HTTP response headers;
- add more specific validation or error messages without changing the error's
  documented HTTP/code class.

Clients must ignore unknown fields, tolerate unknown enum numbers, switch on
oneofs defensively, and use capability discovery before relying on a newly added
feature. An additive schema change does not require a new URL version.

Denpie will not reuse a removed protobuf field number or enum number. Removed
members are reserved in the schema. Changing an existing field's wire type,
meaning, authentication requirement, or successful result shape is breaking and
requires a new major API path such as `/api/v2`.

## Deprecation process

A breaking replacement will be documented in the
[API changelog](api-changelog.md) and advertised through discovery. The old
major version will remain available for at least 90 days and two tagged Denpie
releases after its replacement is production-ready, whichever is longer. A
security issue may require faster restriction; such an exception will be called
out explicitly with migration guidance.

Individual additive operations or fields may be marked deprecated before a
major-version transition. Deprecated members remain wire-compatible for the
lifetime of their API major version unless a documented security exception
applies.

## Compatibility endpoint

`POST /api` is a legacy compatibility surface. It remains available with its
unwrapped `ApiRequest`, body authentication, plain-text errors, and
non-idempotent mutation behavior. It receives compatible operation additions,
but no new transport guarantees. There is currently no removal date. New
clients should not adopt it.

## Schema distribution

[`proto/denpie.proto`](../proto/denpie.proto) is the canonical source schema.
`just api-schema` produces a self-contained `target/api-schema/v1/` bundle with
the source schema, a binary descriptor set, a SHA-256 checksum, and a manifest.
CI publishes the same bundle as the `denpie-api-v1-schema` artifact.

Consumers should pin a tagged Denpie release or a schema-bundle checksum for
reproducible code generation. Tracking the repository's default branch opts
into additive changes as they land.
