// SPDX-License-Identifier: GPL-3.0-only
use super::*;

fn eur() -> Currency {
    Currency::new(*b"EUR").unwrap()
}

fn window() -> ObservationWindow {
    ObservationWindow {
        now_unix_seconds: 1_000,
        maximum_age_seconds: 100,
    }
}

fn observation(id: &str, roic_ppm: i128, fcf_yield_ppm: i128) -> Observation {
    Observation {
        asset_id: id.into(),
        roic_ppm,
        free_cash_flow_yield_ppm: fcf_yield_ppm,
        metric_policy_id: METRIC_POLICY_ID.into(),
        currency: eur(),
        as_of_unix_seconds: 950,
    }
}

fn snapshot() -> OwnershipSnapshot {
    OwnershipSnapshot {
        as_of_unix_seconds: 950,
        total_issued_units: Some(100),
        aggregated_held_units: Some(10),
        reserved_pending_units: Some(2),
    }
}

#[test]
fn yield_scores_normalize_exactly_and_keep_zero_weight() {
    let items = [
        observation("A", 100_000, 3),
        observation("B", 200_000, 1),
        observation("C", 1, 0),
    ];
    let Portfolio::ResearchWeights {
        total_score,
        weights,
        excluded,
    } = research_weights(&items, eur(), window()).unwrap()
    else {
        panic!("positive score required")
    };
    assert_eq!(total_score, 500_000);
    assert_eq!(weights[0].weight, Rational::new(3, 5).unwrap());
    assert_eq!(weights[1].weight, Rational::new(2, 5).unwrap());
    assert_eq!(weights[2].weight, Rational::new(0, 1).unwrap());
    assert!(excluded.is_empty());
}

#[test]
fn negative_times_negative_is_excluded_not_a_positive_signal() {
    let items = [
        observation("A", -10, -50),
        observation("B", 2, -4),
        observation("C", -3, 2),
        observation("D", 1, 1),
    ];
    let Portfolio::ResearchWeights {
        weights, excluded, ..
    } = research_weights(&items, eur(), window()).unwrap()
    else {
        panic!("D is positive")
    };
    assert_eq!(weights.len(), 1);
    assert_eq!(weights[0].asset_id, "D");
    assert_eq!(
        excluded.iter().map(|item| item.reason).collect::<Vec<_>>(),
        vec![
            ExclusionReason::BothNegative,
            ExclusionReason::NegativeCashFlowYield,
            ExclusionReason::NegativeRoic
        ]
    );
}

#[test]
fn zero_and_empty_inputs_explicitly_have_no_portfolio() {
    let Portfolio::NoPortfolio {
        zero_score_assets,
        excluded,
    } = research_weights(
        &[observation("zero", 0, 4), observation("negative", -1, 4)],
        eur(),
        window(),
    )
    .unwrap()
    else {
        panic!("cannot normalize zero")
    };
    assert_eq!(zero_score_assets, ["zero"]);
    assert_eq!(excluded.len(), 1);
    assert!(matches!(
        research_weights(&[], eur(), window()),
        Ok(Portfolio::NoPortfolio { .. })
    ));
}

#[test]
fn metadata_is_validated_even_for_ineligible_rows() {
    assert_eq!(Currency::new(*b"eur"), Err(Error::InvalidCurrencyLabel));
    assert_eq!(
        research_weights(&[observation("", 1, 2)], eur(), window()),
        Err(Error::InvalidAssetId)
    );
    assert_eq!(
        research_weights(
            &[observation("A", 1, 2), observation("A", -1, 2)],
            eur(),
            window()
        ),
        Err(Error::DuplicateAsset)
    );
    let mut item = observation("A", -1, 2);
    item.currency = Currency::new(*b"USD").unwrap();
    assert_eq!(
        research_weights(&[item], eur(), window()),
        Err(Error::CurrencyMismatch)
    );
    let mut item = observation("A", -1, 2);
    item.as_of_unix_seconds = 899;
    assert_eq!(
        research_weights(&[item.clone()], eur(), window()),
        Err(Error::StaleObservation)
    );
    item.as_of_unix_seconds = 1_001;
    assert_eq!(
        research_weights(&[item.clone()], eur(), window()),
        Err(Error::FutureObservation)
    );
    item.as_of_unix_seconds = 900;
    assert!(research_weights(&[item.clone()], eur(), window()).is_ok());
    item.metric_policy_id = "absolute-fcf-v1".into();
    assert_eq!(
        research_weights(&[item], eur(), window()),
        Err(Error::UnsupportedMetricPolicy)
    );
}

#[test]
fn multiplication_and_total_overflow_are_errors() {
    assert_eq!(
        research_weights(&[observation("A", i128::MAX, 3)], eur(), window()),
        Err(Error::ScoreOverflow)
    );
    assert_eq!(
        research_weights(
            &[observation("A", i128::MAX, 2), observation("B", 1, 2)],
            eur(),
            window()
        ),
        Err(Error::TotalScoreOverflow)
    );
}

#[test]
fn strict_threshold_counts_aggregated_holdings_and_pending_reservations() {
    let threshold = Rational::new(1, 5).unwrap();
    let below = ownership_headroom(snapshot(), 7, threshold, window()).unwrap();
    assert_eq!(below.projected_units, 19);
    assert_eq!(below.additional_units_before_proposal, 7);
    assert_eq!(
        below.outcome,
        ThresholdOutcome::StrictlyBelowConfiguredThreshold
    );
    let equal = ownership_headroom(snapshot(), 8, threshold, window()).unwrap();
    assert_eq!(equal.projected_fraction, threshold);
    assert_eq!(
        equal.outcome,
        ThresholdOutcome::AtOrAboveConfiguredThreshold
    );
    let above = ownership_headroom(snapshot(), 9, threshold, window()).unwrap();
    assert_eq!(
        above.outcome,
        ThresholdOutcome::AtOrAboveConfiguredThreshold
    );
}

#[test]
fn discrete_headroom_uses_explicit_strict_rounding_not_float() {
    let snapshot = OwnershipSnapshot {
        total_issued_units: Some(10),
        aggregated_held_units: Some(0),
        reserved_pending_units: Some(0),
        ..snapshot()
    };
    let threshold = Rational::new(1, 3).unwrap();
    assert_eq!(
        ownership_headroom(snapshot, 3, threshold, window())
            .unwrap()
            .outcome,
        ThresholdOutcome::StrictlyBelowConfiguredThreshold
    );
    assert_eq!(
        ownership_headroom(snapshot, 4, threshold, window())
            .unwrap()
            .outcome,
        ThresholdOutcome::AtOrAboveConfiguredThreshold
    );
    assert_eq!(
        ownership_headroom(snapshot, 0, threshold, window())
            .unwrap()
            .maximum_units_strictly_below_threshold,
        3
    );
}

#[test]
fn unknown_stale_or_invalid_ownership_cannot_be_accepted() {
    let threshold = Rational::new(1, 5).unwrap();
    for (snapshot, expected) in [
        (
            OwnershipSnapshot {
                total_issued_units: None,
                ..snapshot()
            },
            Error::UnknownIssuedUnits,
        ),
        (
            OwnershipSnapshot {
                aggregated_held_units: None,
                ..snapshot()
            },
            Error::UnknownAggregatedHoldings,
        ),
        (
            OwnershipSnapshot {
                reserved_pending_units: None,
                ..snapshot()
            },
            Error::UnknownReservedOrders,
        ),
        (
            OwnershipSnapshot {
                total_issued_units: Some(0),
                ..snapshot()
            },
            Error::ZeroIssuedUnits,
        ),
        (
            OwnershipSnapshot {
                as_of_unix_seconds: 899,
                ..snapshot()
            },
            Error::StaleObservation,
        ),
    ] {
        assert_eq!(
            ownership_headroom(snapshot, 0, threshold, window()),
            Err(expected)
        );
    }
    assert_eq!(Rational::new(1, 0), Err(Error::ZeroDenominator));
    for invalid in [Rational::new(0, 1).unwrap(), Rational::new(2, 1).unwrap()] {
        assert_eq!(
            ownership_headroom(snapshot(), 0, invalid, window()),
            Err(Error::InvalidOwnershipThreshold)
        );
    }
}

#[test]
fn ownership_overflow_refuses_instead_of_wrapping() {
    let original = snapshot();
    let huge = OwnershipSnapshot {
        total_issued_units: Some(u128::MAX),
        ..original
    };
    assert_eq!(
        ownership_headroom(huge, 0, Rational::new(2, 3).unwrap(), window()),
        Err(Error::OwnershipArithmeticOverflow)
    );
    let huge = OwnershipSnapshot {
        aggregated_held_units: Some(u128::MAX),
        ..original
    };
    assert_eq!(
        ownership_headroom(huge, 0, Rational::new(1, 5).unwrap(), window()),
        Err(Error::OwnershipArithmeticOverflow)
    );
    assert_eq!(
        ownership_headroom(original, u128::MAX, Rational::new(1, 5).unwrap(), window()),
        Err(Error::OwnershipArithmeticOverflow)
    );
}
