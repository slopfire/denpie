# Denpie API v1 schema bundle

The canonical schema is [`proto/denpie.proto`](../../../proto/denpie.proto).
Generate a reproducible consumer bundle with:

```bash
just api-schema
```

The output is `target/api-schema/v1/` and contains:

- `denpie.proto` — self-contained source schema;
- `denpie.pb` — binary `FileDescriptorSet` with source information;
- `manifest.json` — API/package metadata;
- `SHA256SUMS` — integrity checks for every bundled file.

CI validates and publishes that directory as the `denpie-api-v1-schema`
artifact. Pin a tagged release or checksum when generating a client for
production use.
