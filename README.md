![Project VOLPAROSSA Bank — golden balance scales, a globe and a network of connections](docs/assets/banner-volparossa-bank.png)

# VOLPAROSSA Bank

**Explore a shared portfolio method while keeping ownership and payment authority explicit.**

How could people coordinate investments without handing ownership of everyone's
holdings to one application operator? VOLPAROSSA Bank is developing that idea
alongside cooperative payments, starting with inspectable portfolio arithmetic
and fictitious **TEST-unit** transfers.

The reusable [VOLPAROSSA Transaction-layer](https://github.com/VOLPAROSSA/volparossa/blob/main/docs/services/TRANSACTION_LAYER.md)
handles signed payment intent and transaction state. Bank owns the portfolio
method and presentation. Help test [restart-safe paper payments](docs/PAPER_PAYMENTS.md)
and make the [method's assumptions](docs/RESEARCH.md) reviewable before any real
financial service is considered.

**Research and local test-value prototypes only. No banking service, customer deposits, trading, custody or real
payments are available.** Portfolio value can fall. This is neither investment
advice nor a deposit guarantee; legal qualification is required before handling
real money or securities.

[Explore the portfolio method and the local payment test on the Bank website](https://volparossa.org/volparossa-bank/).

## Portfolio method

The confirmed starting rule is `score = ROIC × FCF-yield`, with neither input negative.
Normalize positive scores to obtain target weights. A zero score receives zero
weight; an all-zero eligible universe does not produce a portfolio.

FCF-yield relates free cash flow to market value, rather than rewarding absolute
cash generation alone. The initial proposed definition is FCF divided by equity
market capitalization; it is not an enterprise-value yield. The reporting period,
ROIC and FCF definitions, valuation time, source provenance and currency conversion
must be consistent before real observations can be compared. This weighting rule
does not guarantee returns or control every investment risk.

The standalone Rust research library calculates exact rational weights and checks
configured ownership headroom, including already reserved purchases. It does not
fetch or authenticate financial reports, verify beneficial ownership, place orders
or determine which legal limits apply.

```sh
cargo test --offline
```

### Preview a fictitious contribution

The Bank CLI shows how a new TEST budget divides into whole units of fictitious
assets. It applies each target weight to the full budget, rounds down at the
supplied TEST price, and limits each purchase to the ownership headroom after
aggregate holdings and reservations. Cash left by caps or rounding stays
unallocated. The preview submits no payments or orders.

For a synthetic snapshot valued at Unix time 1000, allow observations up to 100
seconds old and keep projected ownership strictly below 1/5 of issued units.

```sh
cargo run --locked --offline -- 100 1000 100 1 5 EUR <<'DATA'
SNAPSHOT invented-fixture-v1 500 800
TEST-A 100000 30000 900 10 100 13 3
TEST-B 100000 10000 950 8 200 0 0
DATA
```

The header records the synthetic source identifier and the common reporting
period's start and end as Unix seconds. In this fixture, ROIC means after-tax
operating profit divided by positive average invested capital. FCF-yield means
operating cash flow less capital expenditure divided by positive equity market
capitalization. Both ratios are entered in parts per million. The source label
is supplied by the caller.

Each asset row supplies the asset ID, ROIC ppm, FCF-yield ppm, snapshot time,
positive whole TEST price, issued units, aggregate units held and reserved
units. The time applies to the entire row. `EUR` labels the supplied accounting
inputs for the ratios. The example uses invented ratios and prices. Its 3/4
and 1/4 target weights produce 3 units of each asset, costing 30 and 24 TEST,
with 46 TEST unallocated. Run with `--help` for the input format.

## Discover a price for a fictitious asset

The offline Bank auction accepts one signed limit order per enrolled TEST participant.
Independent readers can calculate the same indicative price from the same roster
and orders. The four-order example crosses seven fictional shares at 10 TEST each.
With no crossing orders the reader reports no price. No funds or shares move.
[Run the example and inspect its matching rules](docs/PAPER_AUCTION.md).

## Core-backed paper payments

The separate `paper-payments` Rust package calls the actual, commit-pinned core
transaction library. It saves an owner's signed intent before submitting it, then
uses the core for reservations, transfers, cancellation and historical receipts.
Bank does not maintain a second balance ledger or turn portfolio scores into money.

After a restart, Bank verifies the retained command and reconciles it with the
core. Retrying submits exactly the original bytes: it cannot quietly change the
amount, recipient, nonce or expiry, or create a second debit. A missing receipt
means "not recorded at this observation," not proof of a failed payment.

Seven focused tests pass, including actual application-process kills before and
after a core commit, reopening, expired retries, conflicting signed commands and
private-file checks. The runnable example ends at 30/70 TEST units after reopening.
These are local application-to-core results, **not distributed settlement**.

The installable `bank-test-authorizer` prepares and reopens signed TEST reservations,
commits and cancellations. Four CometBFT validators have executed the Bank reserve
and commit flow, preserved all 100 fictitious units across a partition and
returned the original receipts after restart and replay.
[Install the owner tool](paper-payments/README.md#authorize-a-command-for-an-external-test-consensus-trial) ·
[Four-validator trial](https://github.com/VOLPAROSSA/volparossa/actions/runs/44534425134) ·
[Run the local example](docs/PAPER_PAYMENTS.md)

## Division of responsibilities

- **Bank application:** portfolio research, position presentation, ownership
  constraints, price discovery and user mandates.
- **Core Transaction-layer:** reusable authorization, reservations, transfer
  state, replay protection, settlement adapters and dispute/correction handling.
- **Existing core layers:** protected communication, permitted public data
  caching, private records and bounded cooperative analysis.

Financial participation is separate from ordinary network participation. Joining
VOLPAROSSA does not invest a user's money, require a financial account or charge
for relay/cache/compute contributions.

## Design and remaining work

[Research and architecture](docs/RESEARCH.md) compares payment protocols, explains
the ownership model, and distinguishes internal corrections from recovery after
external settlement. The developing core contract is documented in
[VOLPAROSSA](https://github.com/VOLPAROSSA/volparossa/blob/main/docs/services/TRANSACTION_LAYER.md).

Independently verified client finality, authenticated positions, actual securities ownership,
external payment gateways, market operation, private AML processing and recovery
of forwarded funds are **not implemented**. Arithmetic and state-machine tests
must not be presented as a working decentralized bank.

Original code is GPL-3.0-only. No third-party payment implementation is bundled.
