# tool-schema

`tool.schema.json` is the JSON Schema for `tools/**/tool.yaml`. It is generated from the Rust types in `crates/core` — do not edit it by hand.

```bash
UPDATE_SCHEMA=1 cargo test -p saqviro-core --test schema
```

It covers structure only. Cross-field rules (presets reference real parameters, id matches category) are enforced by `saqviro-core`.
