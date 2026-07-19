# Lua scripting

RuDS utilities, transforms, and macros run in a sandbox with explicit project-scoped capabilities under `bds`. Start with the complete [API reference](API_REFERENCE.md), [canonical types](TYPES.md), and [runnable examples](examples/). Portable method signatures match bDS2 contract `0.4.0`; RuDS-only helpers are marked as extensions.

`api.json` is the sole method/type manifest. Regenerate the reference and completion data with:

```sh
cargo run -p bds-core --example generate_scripting_docs
```

The core scripting tests verify the Allium capability names, runtime tables, generated files, sandbox, failure values, project scoping, examples, and the frozen bDS2 signature baseline in `bds2-core-signatures.json` together.
