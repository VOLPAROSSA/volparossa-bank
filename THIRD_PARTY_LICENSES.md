# Dependency provenance

Original Bank code is GPL-3.0-only. The root arithmetic prototype has no third-party
Rust dependencies. The separate `paper-payments` adapter does; it neither copies
nor modifies their upstream sources or license notices.

## VOLPAROSSA core

The adapter depends on the GPL-3.0-only `volparossa-transaction` library from
[VOLPAROSSA core](https://github.com/VOLPAROSSA/volparossa), including its
`volparossa-protocol` and `volparossa-core` dependencies. The exact revision is
recorded in `paper-payments/Cargo.toml` and `paper-payments/Cargo.lock`; no unpinned
branch or alternate local implementation is used. Upstream license texts and
notices remain unchanged. There are no Bank-local patches to the imported core.

## Registry dependencies

Direct adapter dependencies are `ed25519-dalek` (BSD-3-Clause), `prost`
(Apache-2.0), `rustix` (Apache-2.0 with LLVM exception, Apache-2.0 or MIT), `sha2`
(MIT or Apache-2.0) and `thiserror` (MIT or Apache-2.0). Test/example dependencies
are `rand_core` and `tempfile` (each MIT or Apache-2.0).

The core also brings its locked transitive dependency graph, including `rusqlite`
and `libsqlite3-sys`. Versions, source identifiers and registry checksums are in
the adapter lockfile. Inspect the resolved upstream package/license metadata with:

```sh
cargo metadata --locked --offline --manifest-path paper-payments/Cargo.toml --format-version 1
```

Fetched crate archives contain their original licenses and notices. No upstream
license is replaced by Bank's license. A future distributable package must collect
the exact applicable notices and source obligations for its locked graph; this
source prototype does not provide a finished redistribution bundle or claim a
complete license audit.
