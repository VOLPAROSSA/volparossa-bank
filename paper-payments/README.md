# Core-backed TEST payment intents

The existing `Intent` API and private version-1 envelope remain owner-local.
`OrderedIntent` is a separate type with a version-2 envelope for Core's distinct
ordered TEST signature domain. Neither type is a financial account or a network,
consensus, finality, custody or real-settlement service.

`OrderedIntent::prepare` signs explicitly authorized terms with a separately
supplied, enrolled financial TEST key and uses the existing bounded private
file/directory/parent fsync path before returning. No key or receipt is saved.
`open` verifies canonical encoding and the ordered ledger, signature, payer and
operation binding through Core. Local and ordered envelopes are not interchangeable.

`submission_bytes(&OrderedStore)` borrows the original signed command after
rechecking its private file and Core binding. Explicit retries use identical bytes,
including the original id, nonce, signature and expiry. There is no auto-resigning
or Bank method to stage/commit a block. Core remains responsible for execution-time
validity, reservations, rejection and permanent replay handling.

`reconcile` reads only committed historical receipts. A successfully staged command
still reports `NotRecorded`; absence is not proof of failure. `Recorded` means the
local ordered executor persisted that original command, not that a client verified
validator finality or real external settlement. Historical reserve receipts remain
historical after a later commit/cancel.

Intent directories/files must remain private (0700/0600), regular/single-link and
unchanged. Checks reject replacement, changed metadata/content, unsafe paths and
noncanonical envelopes. These are bounded admission snapshots, not continuous
monitoring, encryption at rest or protection from a device administrator. Files
left by failed creation are never automatically adopted or submitted.

On an approved disposable build runner, the existing CI can exercise both APIs:

```sh
cargo +1.98.1 test --locked --offline --manifest-path paper-payments/Cargo.toml --all-targets
cargo +1.98.1 run --locked --offline --manifest-path paper-payments/Cargo.toml --example ordered_transfer -- /absolute/path/to/new-test-fixture
```

The example supplies its own fixed TEST block order and agreed timestamps; it does
not run CometBFT or claim distributed agreement. It retains the private fictitious
fixture, but saves no signing key. The tests separately kill and join an owned Bank
process after staging and after Core commit, then reopen and reconcile without a
second debit. Existing local-mode tests continue to cover their original contract.
