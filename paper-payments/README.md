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

## Authorize a command for an external TEST consensus trial

Install the owner’s TEST signing tool from the Bank checkout with Rust 1.98.1 and the Protobuf compiler. The `owner-cli` feature enables the pinned Core ABCI genesis reader. The existing `ordered_authorizer` example accepts the same commands.

```sh
cargo +1.98.1 install --locked --path paper-payments --features owner-cli --bin bank-test-authorizer
bank-test-authorizer --help
```

Use a canonical absolute owner directory with permissions 0700. Its initial contents must be empty.
Identifiers are 64 lowercase hexadecimal characters. Amounts use whole TEST units and validity
bounds use Unix seconds.

| Command arguments | Standard input | Standard output |
| --- | --- | --- |
| `init OWNER LEDGER` | Public ABCI app-state genesis JSON, up to 16,384 bytes | `ready` |
| `reserve OWNER LEDGER OP VALID_FROM EXPIRES RECIPIENT UNITS` | Exactly 32 raw bytes of an enrolled payer's TEST signing seed | Original signed reserve bytes |
| `commit OWNER LEDGER OP VALID_FROM EXPIRES RESERVATION` | Exactly 32 raw bytes of the enrolled payer's TEST signing seed | Original signed commit bytes |
| `cancel OWNER LEDGER OP VALID_FROM EXPIRES RESERVATION` | Exactly 32 raw bytes of the enrolled payer’s TEST signing seed | Original signed cancellation bytes |
| `replay OWNER LEDGER OP` | None | The original signed bytes for the retained operation |

Initialization runs Core's ABCI `InitChain` and checks the resulting ledger against the caller's
independently observed ledger. The owner context stays at genesis height zero. Each authorization
persists a private intent before releasing bytes. Replay reopens that intent and requires only
the ledger and original operation ID. Either commit or cancel can name an existing reservation.
Cancel requests the release of its reserved TEST units to the payer. The validators determine
the result when they order the command. Replaying a saved cancellation returns its original
bytes even after the signing seed has been removed. The trial driver submits the bytes to the validators and
observes their committed blocks and receipts. Keep the signed bytes inside the private trial.
