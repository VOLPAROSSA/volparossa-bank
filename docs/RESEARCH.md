# Bank research and transaction architecture

Research date: 2026-10-04. This document records the requested design, initial
technical choices and unresolved decisions. It is not legal or investment advice,
a launch approval, or evidence of a working financial service.

## The intended system

Participants retain enforceable interests in a portfolio selected with nonnegative
ROIC and FCF-yield. They can exchange value internally and, through suitable
gateways, with the existing financial system. The cooperative immune system should
detect abuse while exposing as little financial information as possible.

Independent internal operation means the network can validate and record agreed
transfers without a single VOLPAROSSA application server. It does **not** mean
that existing listed shares, legal title, corporate actions or external currency
settlement become independent of issuers and financial infrastructure.

## Portfolio construction and actual ownership

The user confirmed **ROIC × FCF-yield**, not absolute FCF. Use
`score_i = ROIC_i × FCF_yield_i` and `weight_i = score_i / sum(scores)`.
Exclude either-negative observations, give zeros no allocation, and return an
explicit no-portfolio result when the denominator is zero.

Before ingesting real reports, specify the universe, reporting period, source and
publication date, market-price timestamp, currency/FX snapshot, and a versioned metric definition. A
candidate ROIC definition is after-tax operating profit divided by average
invested capital, requiring a positive, meaningful denominator. A candidate FCF
definition is operating cash flow minus capital expenditure. These are proposals,
not a claim that all issuers report comparable measures. The SEC specifically
notes that FCF lacks a uniform definition. The initial proposed yield is this FCF
divided by a positive equity market capitalization, with numerator and denominator
in the same currency. Enterprise-value yield would require a different definition.
The arithmetic prototype accepts precomputed ratios; it does not validate their
derivation or retrieve financial statements.
[SEC guidance, question 102.07](https://www.sec.gov/rules-regulations/staff-guidance/corporation-finance-interpretations/non-gaap-financial-measures)

This rule combines a profitability metric and a valuation proxy; it is not a
complete valuation or risk assessment and cannot guarantee future returns. It can
create sector concentration, turnover and sensitivity to accounting adjustments.
Backtests must use information actually available at the decision time, include
delisted companies and realistic costs/taxes, and report drawdown and liquidity,
not only return. Risk caps, rebalancing frequency and treatment of missing/stale
inputs remain explicit design decisions.

**Preferred ownership research direction:** individually attributed holdings and
separate user mandates following a shared methodology. Do not silently replace
those holdings with unsecured claims on other participants. If one transferable
basket unit is desired, research a separate legally enforceable structure; a
fraction recorded in software alone does not establish title, voting or dividend
rights. Record the instrument, issuer, custodian/registrar, beneficial holder,
fractional-rights terms and settlement state. Pooling several investors' capital
under one investment policy can instead fall within fund regulation.
[CSDR, articles 3 and 38](https://eur-lex.europa.eu/legal-content/EN/TXT/?uri=CELEX%3A32014R0909),
[AIFMD, article 4](https://eur-lex.europa.eu/legal-content/EN/TXT/?uri=CELEX%3A02011L0061-20260416)

Ownership caps must track the **aggregated beneficial interest and voting rights**,
not each wallet, device or nominal account separately. Include existing external
holdings, controlled entities, applicable concert-party attribution, pending orders
and changes in issued capital. Unknown totals cannot be called compliant. The AFM
guide describes a 3% initial notification threshold for relevant issuers, with
scope and attribution qualifications; it is not a universal exemption from tax,
AML or investment-service duties. The implementation accepts a researched limit
as input instead of hardcoding a legal safe harbor. Caps may avoid crossing a
threshold lawfully; account splitting to hide an actual crossing is not a feature.
[AFM shareholder guide](https://www.afm.nl/~/profmedia/files/wet-regelgeving/beleidsuitingen/leidraden/leidraad-aandeelhouders.pdf)

## Payments and price discovery

Portfolio weights and market prices are different. An internal payment needs an
explicit asset/unit, amount, quote source/time, fees, slippage bound and expiry.
A euro display value must not promise euro redemption of volatile investments.
Converting an investment into outside money requires an actual buyer or funded
liquidity provider; markets can close and counterparties can fail.

The proposed internal design transfers precisely defined funded entitlements and
nets many small obligations before expensive external settlement. It must show
counterparty exposure and require sufficient collateral/reserves; netting is not
permission to spend the same asset twice. Micro-payments should not require a
stock-market order for every cent. Securities delivery and payment need coupled
settlement, or explicit escrow and reconciliation when a gateway cannot provide
atomic delivery-versus-payment. The core never equates a received message with
settled funds. During a partition an unfunded or unconfirmed payment stays pending.

For independent price discovery, investigate frequent **uniform-price batch
auctions** with signed orders, independently checked matching and a documented
tie-breaking rule. Order confidentiality, censorship resistance and prevention
of front-running require separate protocol work; broadcasting orders is not
private price discovery. With no matching liquidity there is no executable price.
For complementary operation, compare independently sourced external quotes and
allow real arbitrage only through authorized funded gateways. Reject stale or
conflicting sources rather than letting a peer majority invent a market price.
Auctions are an application design candidate, not a change to overlay packet
scheduling. [Budish, Cramton and Shim](https://cramton.umd.edu/papers2010-2014/budish-cramton-shim-frequent-batch-auctions-aerpp.pdf)

## Open protocol comparison

These are research candidates, not installed dependencies or a final stack.

| Candidate | Useful role | Boundary that remains |
| --- | --- | --- |
| Interledger ILPv4, STREAM and Open Payments | Payment routing, small payment packets, quotes and authorization across account systems. | Settlement, funding and connector exposure remain outside ILP; it does not create a securities ledger. |
| GNU Taler | Privacy-friendly payments using existing currencies, including wallet-to-wallet payments. | Requires an exchange/payment provider; not autonomous securities custody or a universal clawback mechanism. |
| Hyperledger Fabric | Reference for endorsed transactions, private data collections and BFT ordering among governed participants. | Permissioned membership and validator independence must be established; privacy is not anonymity and no financial license is supplied. |

**Initial recommendation:** investigate Interledger/Open Payments as the external
interoperability boundary, compare Taler for a distinct spendable payment wallet,
and evaluate established BFT ledger implementations for internal settlement before
choosing one. No mandatory blockchain, speculative network token or gas asset is
selected. Ordinary DHT replication and signatures do not prevent double spending.

ILPv4 uses authenticated peers and funded account relationships; STREAM handles
payment packetization. Settlement is a separate concern, not an ILP guarantee.
[ILPv4](https://interledger.org/developers/rfcs/interledger-protocol/),
[STREAM](https://interledger.org/developers/rfcs/stream-protocol/),
[Interledger stack](https://interledger.org/tech/overview/)

Taler protects payer unlinkability using blind signatures, but wallet-to-wallet
transfers still involve its payment provider. Its own documentation warns that
lost/stolen anonymized cash cannot simply be recovered by the exchange. This
makes it a useful candidate for a bounded payment wallet, not proof that all
privacy and reversal requirements can coexist automatically.
[Taler FAQ](https://www.taler.net/en/faq.html), [Taler documentation](https://docs.taler.net/)

Fabric documents both private data collections and BFT ordering. Its committed
ledger history is final: an application correction would be a new authorized
entry, not deleting the original transaction. Before adoption, check exact source
revision, component licenses, Rust/process integration, operational resources and
independent validator assumptions; no upstream binary is trusted merely because
a project is open source.
[Fabric private data](https://hyperledger-fabric.readthedocs.io/en/latest/private-data/private-data.html),
[Fabric ordering](https://hyperledger-fabric.readthedocs.io/en/latest/orderer/ordering_service.html)

## Immune oversight with financial privacy

Use the seven agreed virtues as context for responsible behavior, not as a
replacement for transaction authorization, explicit legal rules or evidence.
Proposed checks cover duplicate spending, forged ownership, wash trading,
coordinated account splitting, account takeover, fraud and unusual flows.
Models produce reasoned signals; a prediction alone is not proof of a crime or
authority to seize funds. Economic and validator concentration also need monitoring.

Encrypt transaction details for the parties and specifically authorized reviewers;
do not publish balances or reusable personal identifiers in the public cache or
training data. Research selective disclosure and established zero-knowledge
proofs for narrow predicates, but do not claim these reveal every suspicious
behavior. Minimal disclosures reduce exposure; they do not make full anonymity,
complete flow tracing and universal reversibility simultaneously available.

Financial gatekeepers still need applicable identity, beneficial-owner,
monitoring, retention and reporting duties. The DNB guidance treats responsibility
for customer acceptance and outsourced monitoring explicitly; delegating model
work does not dissolve that responsibility. Significant automatic financial
decisions also need lawful grounds and applicable intervention/contest safeguards.
[DNB Wwft guidance](https://www.dnb.nl/media/meidddba/dnb-qas-en-good-practices-wwft.pdf),
[GDPR, articles 5, 22 and 25](https://eur-lex.europa.eu/legal-content/EN/TXT/?uri=CELEX%3A32016R0679)

## Recovery after forwarding or splitting

The intended recovery mechanism is an auditable case workflow, not a master key
that allows an agent to rewrite every balance.

1. Preserve the original authorization and transaction lineage in protected
   records. A report identifies disputed value, evidence, applicable authority
   and deadline; it does not mark all downstream holders guilty.
2. Where allowed, place scoped, expiring holds on still-controllable value.
   Forwarding/splitting must not create or multiply the recovery claim. Recover
   only up to the outstanding authorized amount across all related operations.
3. Require independently authorized adjudication, including applicable contest
   and intervention rights. A compromised model or a collection of fake peers
   must not grant itself confiscation authority.
4. Append compensating transfers backed by available funds or an explicitly
   funded reserve. Keep the original transactions and correction reasons;
   never fabricate recovered value or silently debit unrelated innocent parties.
5. For externally final transfers, issue a provider/legal recovery request and
   track its outcome. Until funds actually return, report a shortfall or claim,
   not a restored balance. Intermediary insolvency and legal protections of
   downstream recipients can prevent recovery.

Flow analysis may help find dispersed funds, but does not create jurisdiction,
access to unknown external accounts or liquidity. The privacy/finality/recovery
trade-off must be visible to participants. External settlement finality is a
real boundary. [CSDR, article 39](https://eur-lex.europa.eu/legal-content/EN/TXT/?uri=CELEX%3A32014R0909)

## Delivery sequence

Start with local arithmetic and transaction-state simulations, then a disposable
multi-peer ledger test using a selected existing protocol. Prove conservation,
concurrent-spend rejection, restart/replay behavior, partition handling and
authorized corrections before connecting a financial gateway. Then add a
paper portfolio, auction simulation and gateway sandboxes with explicit fake
assets. Gateways handling real funds remain disabled until their legal structure,
authority and custody arrangements are established.

The project's name does not determine its regulatory classification. Receiving
funds, providing payments, managing investments and operating a trading venue
need separate assessment. Research can proceed without pretending those services
are already permitted. [DNB bank definition](https://www.dnb.nl/voor-de-sector/open-boek-toezicht/sectoren/banken/vergunningaanvraag-banken/definitie-bank/),
[DNB payment services](https://www.dnb.nl/voor-de-sector/open-boek-toezicht/sectoren/betaalinstellingen/vergunningaanvraag-betaaldiensten-overzichtspagina/wettelijk-kader-markttoegang-betaaldienstverleners/),
[MiFID II](https://eur-lex.europa.eu/eli/dir/2014/65/2026-06-06/eng)
