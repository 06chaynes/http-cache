# List available just recipes
@help:
    just -l

# Run tests on all crates with proper feature combinations using nextest
@test:
    echo "----------\nCore library (smol):\n"
    cd http-cache && cargo nextest run --no-default-features --features manager-cacache,cacache-smol,with-http-types,manager-moka,streaming-smol
    echo "\n----------\nCore library (tokio):\n"
    cd http-cache && cargo nextest run --no-default-features --features manager-cacache,cacache-tokio,with-http-types,manager-moka,streaming-tokio
    echo "\n----------\nReqwest middleware:\n"
    cd http-cache-reqwest && cargo nextest run --all-features
    echo "\n----------\nSurf middleware:\n"
    cd http-cache-surf && cargo nextest run --all-features
    echo "\n----------\nTower middleware:\n"
    cd http-cache-tower && cargo nextest run --all-features
    echo "\n----------\nQuickcache middleware:\n"
    cd http-cache-quickcache && cargo nextest run --all-features

# Run doctests on all crates with proper feature combinations
@doctest:
    echo "----------\nCore library (smol):\n"
    cd http-cache && cargo test --doc --no-default-features --features manager-cacache,cacache-smol,with-http-types,manager-moka,streaming-smol
    echo "\n----------\nCore library (tokio):\n"
    cd http-cache && cargo test --doc --no-default-features --features manager-cacache,cacache-tokio,with-http-types,manager-moka,streaming-tokio
    echo "\n----------\nReqwest middleware:\n"
    cd http-cache-reqwest && cargo test --doc --all-features
    echo "\n----------\nSurf middleware:\n"
    cd http-cache-surf && cargo test --doc --all-features
    echo "\n----------\nTower middleware:\n"
    cd http-cache-tower && cargo test --doc --all-features
    echo "\n----------\nQuickcache middleware:\n"
    cd http-cache-quickcache && cargo test --doc --all-features

@check:
    echo "----------\nCore library (smol):\n"
    cd http-cache && cargo check --no-default-features --features manager-cacache,cacache-smol,with-http-types,manager-moka,streaming-smol
    echo "\n----------\nCore library (tokio):\n"
    cd http-cache && cargo check --no-default-features --features manager-cacache,cacache-tokio,with-http-types,manager-moka,streaming-tokio
    echo "\n----------\nReqwest middleware:\n"
    cd http-cache-reqwest && cargo check --all-features
    echo "\n----------\nSurf middleware:\n"
    cd http-cache-surf && cargo check --all-features
    echo "\n----------\nTower middleware:\n"
    cd http-cache-tower && cargo check --all-features
    echo "\n----------\nQuickcache middleware:\n"
    cd http-cache-quickcache && cargo check --all-features


# Run benchmarks with `cargo bench`
@bench:
    echo "----------\nCore library (smol):\n"
    cd http-cache && cargo bench --no-default-features --features manager-cacache,cacache-smol,with-http-types,manager-moka
    echo "\n----------\nCore library (tokio):\n"
    cd http-cache && cargo bench --no-default-features --features manager-cacache,cacache-tokio,with-http-types,manager-moka
    echo "\n----------\nTower middleware:\n"
    cd http-cache-tower && cargo bench --all-features

# Run benchmarks with `cargo criterion`
@criterion:
    echo "----------\nCore library (smol):\n"
    cd http-cache && cargo criterion --no-default-features --features manager-cacache,cacache-smol,with-http-types,manager-moka
    echo "\n----------\nCore library (tokio):\n"
    cd http-cache && cargo criterion --no-default-features --features manager-cacache,cacache-tokio,with-http-types,manager-moka
    echo "\n----------\nTower middleware:\n"
    cd http-cache-tower && cargo criterion --all-features

# Run the streaming memory profile examples
# Run memory profiling example to compare streaming vs traditional caching
memory-profile:
    cd http-cache-tower && cargo run --release --example streaming_memory_profile --features streaming
    cd http-cache-reqwest && cargo run --release --example streaming_memory_profile --features streaming

# Run Tower Hyper examples
@examples:
    echo "----------\nTower Hyper Basic Example:\n"
    cd http-cache-tower && cargo run --example hyper_basic --features manager-cacache
    echo "\n----------\nTower Hyper Streaming Example:\n"
    cd http-cache-tower && cargo run --example hyper_streaming --features manager-cacache

# Generate a changelog with git-cliff
changelog TAG:
    git-cliff --prepend CHANGELOG.md -u --tag {{TAG}}

# Install workspace tools
@install-tools:
    cargo install cargo-nextest
    cargo install git-cliff
    cargo install cargo-msrv
    cargo install cargo-criterion

# Lint all crates with clippy and check formatting
@lint:
    echo "----------\nCore library (smol):\n"
    cd http-cache && cargo clippy --lib --tests --all-targets --no-default-features --features manager-cacache,cacache-smol,with-http-types,manager-moka,streaming-smol -- -D warnings
    echo "\n----------\nCore library (tokio):\n"
    cd http-cache && cargo clippy --lib --tests --all-targets --no-default-features --features manager-cacache,cacache-tokio,with-http-types,manager-moka,streaming-tokio -- -D warnings
    echo "\n----------\nReqwest middleware:\n"
    cd http-cache-reqwest && cargo clippy --lib --tests --all-targets --all-features -- -D warnings
    echo "\n----------\nSurf middleware:\n"
    cd http-cache-surf && cargo clippy --lib --tests --all-targets --all-features -- -D warnings
    echo "\n----------\nTower middleware:\n"
    cd http-cache-tower && cargo clippy --lib --tests --all-targets --all-features -- -D warnings
    echo "\n----------\nQuickcache middleware:\n"
    cd http-cache-quickcache && cargo clippy --lib --tests --all-targets --all-features -- -D warnings
    echo "\n----------\nFormatting check:\n"
    cargo fmt -- --check

# Format all crates using cargo fmt
@fmt:
    cargo fmt --all

# Find MSRV for all crates
@msrv-find:
    echo "----------\nCore library:\n"
    cd http-cache && cargo msrv find
    echo "\n----------\nReqwest middleware:\n"
    cd http-cache-reqwest && cargo msrv find
    echo "\n----------\nSurf middleware:\n"
    cd http-cache-surf && cargo msrv find
    echo "\n----------\nTower middleware:\n"
    cd http-cache-tower && cargo msrv find
    echo "\n----------\nQuickcache middleware:\n"
    cd http-cache-quickcache && cargo msrv find

# Verify MSRV for all crates
@msrv-verify:
    echo "----------\nCore library:\n"
    cd http-cache && cargo msrv verify
    echo "\n----------\nReqwest middleware:\n"
    cd http-cache-reqwest && cargo msrv verify
    echo "\n----------\nSurf middleware:\n"
    cd http-cache-surf && cargo msrv verify
    echo "\n----------\nTower middleware:\n"
    cd http-cache-tower && cargo msrv verify
    echo "\n----------\nQuickcache middleware:\n"
    cd http-cache-quickcache && cargo msrv verify

# Dry run cargo publish for core library
@dry-publish-core:
    echo "----------\nCore library:\n"
    cd http-cache && cargo publish --dry-run

# Dry run cargo publish for middleware crates
@dry-publish-middleware:
    echo "----------\nMiddleware crates:\n"
    echo "Reqwest middleware:"
    cd http-cache-reqwest && cargo publish --dry-run
    echo "Surf middleware:"
    cd http-cache-surf && cargo publish --dry-run
    echo "Tower middleware:"
    cd http-cache-tower && cargo publish --dry-run
    echo "Quickcache middleware:"
    cd http-cache-quickcache && cargo publish --dry-run

# Dry run cargo publish for all crates to check what would be published
@dry-publish: dry-publish-core dry-publish-middleware

# Publish core library
@publish-core:
    echo "----------\nCore library:\n"
    cd http-cache && cargo publish

# Publish middleware crates
@publish-middleware:
    echo "----------\nMiddleware crates:\n"
    echo "Reqwest middleware:"
    cd http-cache-reqwest && cargo publish
    echo "Surf middleware:"
    cd http-cache-surf && cargo publish
    echo "Tower middleware:"
    cd http-cache-tower && cargo publish
    echo "Quickcache middleware:"
    cd http-cache-quickcache && cargo publish

# Publish all crates using cargo publish (core library first, then middleware)
@publish: publish-core publish-middleware