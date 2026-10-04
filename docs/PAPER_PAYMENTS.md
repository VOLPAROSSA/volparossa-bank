# Core-backed paper payments

This prototype connects Bank to the real VOLPAROSSA transaction library with
fictitious `volparossa.test.unit.v1` units. It demonstrates how an application can
retain an authorized command, survive interruption and discover the original
result without issuing another payment. It is not a wallet, bank service, network
settlement protocol or connection to a financial provider.

## What runs

Bank owns an immutable **intent file**: the original signed command, ledger and
operation identifiers, and the hash of those exact bytes. The core owns the only
balance and transaction store. Its separately enrolled financial test keys are
unrelated to ordinary network participation.

The sequence is:

1. The owner explicitly supplies a command and signing key. Bank asks the core to
   validate the signed terms and saves the intent durably before execution.
2. Bank submits those retained bytes. The core atomically changes its local
   balances and records the command's historical receipt.
3. On reopening, Bank verifies the retained signature and its binding to the
   ledger and operation. It reads the original result from the core.
4. An explicit retry submits the same bytes. It does not renew an expired command,
   invent a new identifier or create another debit for an already accepted command.

Reserving, committing and cancelling are separate signed owner actions. A commit
credits the original recipient; cancellation only returns an uncommitted
reservation. Neither an agent recommendation nor a portfolio weight authorizes
these actions. A historical reservation receipt is not a statement that the
reservation is still pending today.

## Run locally

The existing root portfolio package remains dependency-free. This adapter has its
own manifest and lockfile and requires Rust 1.98.1 on Linux. Fetching dependencies
needs Internet access once; execution uses no network or external accounts.

```sh
cargo +1.98.1 fetch --locked --manifest-path paper-payments/Cargo.toml
cargo +1.98.1 test --locked --offline --manifest-path paper-payments/Cargo.toml --all-targets
cargo +1.98.1 run --locked --offline --manifest-path paper-payments/Cargo.toml --example paper_transfer -- /absolute/path/to/new-paper-payment
```

The example requires a new absolute directory with an existing parent. It refuses
an existing directory rather than overwriting or adopting it. It creates an
explicit 100-unit test balance, transfers 70, reopens both the intent and core
store, reconciles the receipt and repeats the accepted command after expiry.
Final available balances are 30 and 70, with zero reserved units.

The example uses a synthetic clock and ephemeral random signing keys. It retains
the private test ledger and intent files but does not save private keys. It is
therefore not an enrollment or key-recovery workflow. Remove the fixture yourself
when no longer needed; never substitute actual financial records or credentials.

## Verified behavior and limits

Seven focused tests exercise the actual pinned core library and its database:

- Reserve, commit, cancel, reopen and exact retry, including after expiry.
- An expired, never-accepted intent remains unrecorded; no implicit re-signing.
- Tampered descriptors, signatures and noncanonical data are rejected before use.
- A different accepted command with the same identifier cannot be mistaken for
  this intent's receipt; retry cannot cause another debit.
- Private, bounded files; no existing-file adoption, symlink or hard-link use.
- No cancellation without a separately supplied, enrolled owner authorization.
- Actual process termination before and after a core commit, followed by
  reconciliation and retry without a duplicate credit. The marked-ignored child
  entrypoint is invoked twice by this test; it is not omitted crash coverage.

Directories use mode 0700 and files 0600. Permissions are not encryption at rest
or protection from the device administrator. Records are not public-cache or
training material. The prototype has no background execution, persisted signing
keys, automatic authorization or second application ledger.

All results are **owner-local**. There is no daemon endpoint, multi-peer ordering,
distributed double-spend prevention, external settlement, portfolio purchase,
financial compliance decision or recovery of real funds. Those remain required
later work; passing this prototype does not complete the Transaction-layer.

## Source and dependency boundaries

`paper-payments/Cargo.toml` fixes the core to an exact Git commit, and its separate
`Cargo.lock` records the complete dependency graph and registry checksums. There
is no floating branch, local path fallback or duplicate transaction implementation.
See [dependency provenance](../THIRD_PARTY_LICENSES.md). The source workflow checks
formatting, lint and the targeted tests; publication of this code is not evidence
that every workflow has passed or that it is ready for real financial use.
