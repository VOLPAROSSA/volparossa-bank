# Replay a signed TEST auction

The Bank reader discovers a uniform price for the fictional asset `TEST-SHARE`,
quoted in whole TEST per share. Give each reader the same independently retained
roster and closed order book. Each can verify every order and recalculate the quote
offline. The reader opens no network connection and creates no payment intent.

From the repository root, build the existing paper-payments crate and run the sample
in a fresh directory. The sample signer keys are public constants for fictional use.

```sh
cargo build --locked --manifest-path paper-payments/Cargo.toml --bin paper-auction
auction=paper-payments/target/debug/paper-auction
trial=$(mktemp -d)
"$auction" sample-roster > "$trial/roster"
"$auction" sample-orders > "$trial/orders"
"$auction" verify "$trial/roster" < "$trial/orders"
```

The buyers bid for seven units at up to 12 TEST and three at up to 9 TEST.
The sellers offer five units at a minimum of 8 TEST and four at a minimum of
10 TEST. Seven units can cross at 10, 11 or 12 TEST. Each of those prices
has two excess units of supply, so the lowest price wins. The output reads
`TEST-SHARE: 7 units at 10 TEST per unit`.

For every positive integer price, executable volume within the submitted orders
is the smaller of eligible supply and demand. Choose maximum volume, then the
smallest absolute imbalance and then the lowest price. Only prices where supply
or demand changes need evaluation. A bid is still eligible at its limit. An
empty book, a single side, or a spread without a crossing produces no price.

## Signed inputs

The roster starts with a nonzero, 32-byte batch identifier followed by one
Ed25519 public key per line. Use lowercase hex and at most 1,024 distinct keys.
An order line has five whitespace-separated fields.

```text
public_key buy|sell units limit_price signature
```

Units and price are positive unsigned 64-bit integers. A participant may submit
one order per batch. An unknown signer, duplicate participant, weak public key,
or invalid signature rejects the whole book. File order has no effect.

For an independent signer, compute the roster SHA-256 over the ASCII string
`VOLPAROSSA-BANK/TEST-SHARE/TEST/roster/v1`, a zero byte, the 32 raw batch
bytes and all raw 32-byte public keys sorted in ascending byte order. Sign
the ASCII string `VOLPAROSSA-BANK/TEST-SHARE/TEST/auction/v1`, a zero byte,
the raw roster digest, the signer's raw public key, one side byte (0 for buy,
1 for sell), then units and limit price as unsigned 64-bit big-endian integers.
The last text field is the 64-byte Ed25519 signature in lowercase hex.

This simulation calculates price and volume on the orders supplied. Enrollment,
timely inclusion, confidential submission, funding, allocation and settlement
remain separate. Keep real financial data out of the fictional roster and book.
