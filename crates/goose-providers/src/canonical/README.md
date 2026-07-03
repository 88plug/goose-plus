# Canonical Model System

Provides a unified view of model metadata (pricing, capabilities, context limits) across different LLM providers. 
Normalizes provider-specific model names (e.g., `claude-3-5-sonnet-20241022`) 
to canonical IDs (e.g., `anthropic/claude-3.5-sonnet`).

## Build Canonical Models
Fetches latest model metadata from models.dev and updates the registry:
```bash
cargo run --bin build_canonical_models
```

This writes to: `crates/goose-providers/src/canonical/data/canonical_models.json`

The script is currently built from `crates/goose/src/bin/build_canonical_models.rs` and writes into this crate's `src/canonical/data` directory.
