// SPDX-License-Identifier: GPL-3.0-only
//! Offline arithmetic research, not a bank, investment recommendation or trading system.
//!
//! The experimental score is `ROIC_ppm × FCF_yield_ppm`, not absolute free cash flow.
//! Both ratios are supplied as signed parts per million. Their source validation,
//! FCF/market-capitalization derivation and economic suitability are not established
//! by this arithmetic prototype. Inputs must already be comparable and attributed.
//! There is no network, broker, order execution, market-data feed or legal inference.

use std::collections::BTreeSet;
use std::num::NonZeroU128;

pub const RESEARCH_ONLY: &str = "Research-only arithmetic; not investment advice, a legal ownership assessment or trade authorization.";

/// Versioned arithmetic input contract. FCF yield means free cash flow divided by
/// positive equity market capitalization, not enterprise value or absolute FCF.
/// Callers supply both ratios in ppm (100_000 = 10%) after a consistent currency,
/// time-period and accounting policy has been applied. This library cannot verify
/// the underlying figures, authenticate them, perform FX conversion or recover
/// precision already lost by the caller when producing integer ppm values.
pub const METRIC_POLICY_ID: &str = "roic-ppm-times-fcf-yield-ppm-v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidCurrencyLabel,
    InvalidAssetId,
    DuplicateAsset,
    CurrencyMismatch,
    UnsupportedMetricPolicy,
    FutureObservation,
    StaleObservation,
    ScoreOverflow,
    TotalScoreOverflow,
    ZeroDenominator,
    InvalidOwnershipThreshold,
    UnknownIssuedUnits,
    UnknownAggregatedHoldings,
    UnknownReservedOrders,
    ZeroIssuedUnits,
    OwnershipArithmeticOverflow,
}

/// A three-uppercase-letter label, not validation against a currency registry.
/// No conversion or exchange-rate assumption is performed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Currency([u8; 3]);

impl Currency {
    pub fn new(label: [u8; 3]) -> Result<Self, Error> {
        if label.iter().all(u8::is_ascii_uppercase) {
            Ok(Self(label))
        } else {
            Err(Error::InvalidCurrencyLabel)
        }
    }

    pub const fn label(self) -> [u8; 3] {
        self.0
    }
}

/// Exact reduced fraction: no floating point or rounding occurs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rational {
    numerator: u128,
    denominator: NonZeroU128,
}

impl Rational {
    pub fn new(numerator: u128, denominator: u128) -> Result<Self, Error> {
        let nonzero = NonZeroU128::new(denominator).ok_or(Error::ZeroDenominator)?;
        let (mut a, mut b) = (numerator, denominator);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        Ok(Self {
            numerator: numerator / a,
            // a is a positive divisor of the nonzero denominator.
            denominator: NonZeroU128::new(nonzero.get() / a).ok_or(Error::ZeroDenominator)?,
        })
    }

    pub const fn numerator(self) -> u128 {
        self.numerator
    }

    pub const fn denominator(self) -> u128 {
        self.denominator.get()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObservationWindow {
    pub now_unix_seconds: u64,
    pub maximum_age_seconds: u64,
}

impl ObservationWindow {
    pub fn validate(self, as_of_unix_seconds: u64) -> Result<(), Error> {
        let age = self
            .now_unix_seconds
            .checked_sub(as_of_unix_seconds)
            .ok_or(Error::FutureObservation)?;
        if age > self.maximum_age_seconds {
            return Err(Error::StaleObservation);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Observation {
    /// Caller-resolved identity. Tickers/share classes are not resolved here.
    pub asset_id: String,
    /// Parts per million; 100_000 denotes 10%, not 100_000 percent.
    pub roic_ppm: i128,
    /// Precomputed FCF/equity-market-capitalization ratio in signed ppm.
    /// Its source denominator must be known and positive; raw source validation
    /// is the caller's responsibility and is not proved by accepting this value.
    pub free_cash_flow_yield_ppm: i128,
    /// Must match METRIC_POLICY_ID; a different yield definition is not accepted.
    pub metric_policy_id: String,
    /// Common source currency label before forming dimensionless ratios.
    /// This is metadata only, not evidence of valid conversion or source figures.
    pub currency: Currency,
    pub as_of_unix_seconds: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExclusionReason {
    NegativeRoic,
    NegativeCashFlowYield,
    BothNegative,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Exclusion {
    pub asset_id: String,
    pub reason: ExclusionReason,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResearchWeight {
    pub asset_id: String,
    /// Product of the supplied ppm values, in ppm squared, not currency units.
    pub raw_score: u128,
    pub weight: Rational,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Portfolio {
    /// Empty input, or no positive eligible score. No zero denominator is invented.
    NoPortfolio {
        zero_score_assets: Vec<String>,
        excluded: Vec<Exclusion>,
    },
    ResearchWeights {
        total_score: u128,
        weights: Vec<ResearchWeight>,
        excluded: Vec<Exclusion>,
    },
}

/// Normalize experimental ROIC-times-FCF-yield scores exactly. Negative rows are excluded;
/// zero rows retain weight zero when a positive portfolio exists. All input metadata
/// is validated, including rows that will subsequently be excluded.
///
/// This does not measure diversification, risk, tradability or future returns.
pub fn research_weights(
    observations: &[Observation],
    currency: Currency,
    window: ObservationWindow,
) -> Result<Portfolio, Error> {
    let mut identities = BTreeSet::new();
    for item in observations {
        if item.asset_id.is_empty()
            || item.asset_id.len() > 128
            || item.asset_id.trim() != item.asset_id
            || item.asset_id.chars().any(char::is_control)
        {
            return Err(Error::InvalidAssetId);
        }
        if !identities.insert(item.asset_id.as_str()) {
            return Err(Error::DuplicateAsset);
        }
        if item.currency != currency {
            return Err(Error::CurrencyMismatch);
        }
        if item.metric_policy_id != METRIC_POLICY_ID {
            return Err(Error::UnsupportedMetricPolicy);
        }
        window.validate(item.as_of_unix_seconds)?;
    }

    let mut excluded = Vec::new();
    let mut scored = Vec::new();
    let mut total_score = 0_u128;
    for item in observations {
        let reason = match (item.roic_ppm < 0, item.free_cash_flow_yield_ppm < 0) {
            (true, true) => Some(ExclusionReason::BothNegative),
            (true, false) => Some(ExclusionReason::NegativeRoic),
            (false, true) => Some(ExclusionReason::NegativeCashFlowYield),
            (false, false) => None,
        };
        if let Some(reason) = reason {
            excluded.push(Exclusion {
                asset_id: item.asset_id.clone(),
                reason,
            });
            continue;
        }
        let roic = u128::try_from(item.roic_ppm).map_err(|_| Error::ScoreOverflow)?;
        let fcf_yield =
            u128::try_from(item.free_cash_flow_yield_ppm).map_err(|_| Error::ScoreOverflow)?;
        let score = roic.checked_mul(fcf_yield).ok_or(Error::ScoreOverflow)?;
        total_score = total_score
            .checked_add(score)
            .ok_or(Error::TotalScoreOverflow)?;
        scored.push((item.asset_id.clone(), score));
    }
    if total_score == 0 {
        return Ok(Portfolio::NoPortfolio {
            zero_score_assets: scored.into_iter().map(|(id, _)| id).collect(),
            excluded,
        });
    }
    let weights = scored
        .into_iter()
        .map(|(asset_id, raw_score)| {
            Ok(ResearchWeight {
                asset_id,
                raw_score,
                weight: Rational::new(raw_score, total_score)?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(Portfolio::ResearchWeights {
        total_score,
        weights,
        excluded,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OwnershipSnapshot {
    pub as_of_unix_seconds: u64,
    /// Comparable indivisible units; e.g. all quantities may use the same
    /// fractional-share unit. Values must already account for share-class rules.
    pub total_issued_units: Option<u128>,
    /// The caller must aggregate the relevant holdings; this function cannot
    /// discover related accounts, beneficial ownership or incomplete reporting.
    pub aggregated_held_units: Option<u128>,
    /// Positive pending commitments, not a netted expectation of future sales.
    pub reserved_pending_units: Option<u128>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThresholdOutcome {
    StrictlyBelowConfiguredThreshold,
    AtOrAboveConfiguredThreshold,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OwnershipHeadroom {
    pub outcome: ThresholdOutcome,
    pub held_and_reserved_units: u128,
    pub projected_units: u128,
    pub projected_fraction: Rational,
    pub maximum_units_strictly_below_threshold: u128,
    pub additional_units_before_proposal: u128,
}

/// Research-only check against a caller-chosen threshold, not a legal or order
/// authorization. Equality is rejected. Unknown or stale inputs are never treated
/// as zero. Pending reservations count before a proposed addition.
///
/// The discrete limit is `floor((issued * numerator - 1) / denominator)`.
/// Subtracting one **before** division enforces strict inequality exactly even
/// when the threshold is fractional. Arithmetic overflow is a refusal, not a cap.
pub fn ownership_headroom(
    snapshot: OwnershipSnapshot,
    proposed_additional_units: u128,
    threshold: Rational,
    window: ObservationWindow,
) -> Result<OwnershipHeadroom, Error> {
    if threshold.numerator() == 0 || threshold.numerator() > threshold.denominator() {
        return Err(Error::InvalidOwnershipThreshold);
    }
    window.validate(snapshot.as_of_unix_seconds)?;
    let issued = snapshot
        .total_issued_units
        .ok_or(Error::UnknownIssuedUnits)?;
    if issued == 0 {
        return Err(Error::ZeroIssuedUnits);
    }
    let held = snapshot
        .aggregated_held_units
        .ok_or(Error::UnknownAggregatedHoldings)?;
    let reserved = snapshot
        .reserved_pending_units
        .ok_or(Error::UnknownReservedOrders)?;
    let committed = held
        .checked_add(reserved)
        .ok_or(Error::OwnershipArithmeticOverflow)?;
    let projected = committed
        .checked_add(proposed_additional_units)
        .ok_or(Error::OwnershipArithmeticOverflow)?;
    let scaled_threshold = issued
        .checked_mul(threshold.numerator())
        .ok_or(Error::OwnershipArithmeticOverflow)?;
    let maximum = scaled_threshold
        .checked_sub(1)
        .ok_or(Error::OwnershipArithmeticOverflow)?
        / threshold.denominator();
    Ok(OwnershipHeadroom {
        outcome: if projected <= maximum {
            ThresholdOutcome::StrictlyBelowConfiguredThreshold
        } else {
            ThresholdOutcome::AtOrAboveConfiguredThreshold
        },
        held_and_reserved_units: committed,
        projected_units: projected,
        projected_fraction: Rational::new(projected, issued)?,
        maximum_units_strictly_below_threshold: maximum,
        additional_units_before_proposal: maximum.saturating_sub(committed),
    })
}

#[cfg(test)]
mod tests;
