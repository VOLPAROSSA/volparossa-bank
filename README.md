![Project VOLPAROSSA Bank — golden balance scales, a globe and a network of connections](docs/assets/banner-volparossa-bank.png)

# Project VOLPAROSSA Bank

A developing application for participant-owned portfolios and cooperative payments,
with portfolio research and a first executable connection to the VOLPAROSSA
Transaction-layer using fictitious TEST units.

The idea is to hold investments with participants, coordinate a common portfolio
method and exchange value through the network. External payment and securities
connections should complement that internal system, without a single application
operator becoming the owner of everyone's investments.

**Research and local test-value prototypes only. No banking service, customer deposits, trading, custody or real
payments are available.** Portfolio value can fall. This is neither investment
advice nor a deposit guarantee; legal qualification is required before handling
real money or securities.

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

[Run the example and understand the boundaries →](docs/PAPER_PAYMENTS.md)

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

Distributed consensus, authenticated positions, actual securities ownership,
external payment gateways, market operation, private AML processing and recovery
of forwarded funds are **not implemented**. Arithmetic and state-machine tests
must not be presented as a working decentralized bank.

Original code is GPL-3.0-only. No third-party payment implementation is bundled.
