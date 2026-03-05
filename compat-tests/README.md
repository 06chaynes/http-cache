# Compatibility Tests: 0.16 -> 1.0-alpha Migration

These tools validate the real-world upgrade path from http-cache-reqwest 0.16.0
(bincode serialization) to 1.0-alpha.4 (postcard serialization).

## What's being tested

| Scenario | Expected Result |
|---|---|
| Old bincode entries read with new code (postcard only) | Graceful cache miss (`Ok(None)`), no crash |
| Old bincode entries read with compat features enabled | Successful deserialization via bincode fallback |
| New postcard entries written and read | Normal cache hit |

## Prerequisites

- Rust toolchain
- Internet access (for the legacy writer to make HTTP requests)

## Step-by-step manual validation

### 1. Build and run the legacy writer

This uses http-cache-reqwest 0.16.0 from crates.io to populate a cache directory
with bincode-serialized entries.

```bash
cd compat-tests/legacy-writer
cargo run -- /tmp/legacy-cache
```

### 2. Read the cache with the new code (bincode fallback)

This uses the current workspace code with `manager-cacache-bincode` and
`http-headers-compat` features to read back the old entries.

```bash
cd compat-tests/read-legacy-cache
cargo run -- /tmp/legacy-cache
```

Expected output: all entries should be HIT (bincode fallback working).

### 3. Verify graceful degradation without compat features

You can also test by modifying `read-legacy-cache/Cargo.toml` to remove
`manager-cacache-bincode` and `http-headers-compat`, then rebuild and run.
Old entries should return as cache misses without errors.

## Automated CI tests

The equivalent scenarios are also tested in the workspace test suite:

```bash
# Test bincode->postcard migration (with compat features)
cargo test -p http-cache --features manager-cacache,manager-cacache-bincode,http-headers-compat cacache_bincode_to_postcard

# Test graceful degradation (postcard only, no bincode feature)
cargo test -p http-cache --features manager-cacache cacache_bincode_entry_read_without_compat

# Full middleware-level e2e test
cargo test -p http-cache-reqwest --features manager-cacache,manager-cacache-bincode,http-headers-compat migration_from_bincode
```

## Key compatibility facts

| | http-cache-reqwest 0.16.0 | http-cache-reqwest 1.0.0-alpha.4 |
|---|---|---|
| http-cache | 0.21.0 | 1.0.0-alpha.4 |
| Serialization | bincode 1.3.3 | postcard 1.1 (default) |
| cacache | 13.1.0 | 13.1.0 (same!) |
| http-cache-semantics | 2.1.0 | 2.1.0 (same!) |
| Cache key format | `METHOD:URI` | `METHOD:URI` (same!) |
