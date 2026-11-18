// SPDX-License-Identifier: GPL-3.0-only
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{self, Read};
use std::num::NonZeroU64;
use std::str::FromStr;

use volparossa_bank_research::{
    Currency, METRIC_POLICY_ID, Observation, ObservationWindow, OwnershipSnapshot, Portfolio,
    Rational, ownership_headroom, research_weights,
};

const USAGE: &str = "Usage: volparossa-bank-research BUDGET_TEST NOW_UNIX MAX_AGE_SECONDS CAP_NUM CAP_DEN METRIC_CURRENCY
Read a fictitious portfolio snapshot from standard input. First give its shared source and reporting period:
SNAPSHOT SOURCE_ID PERIOD_START_UNIX PERIOD_END_UNIX
Then give one whitespace-separated row per asset:
TEST-ID ROIC_PPM FCF_YIELD_PPM SNAPSHOT_UNIX PRICE_TEST ISSUED AGGREGATED_HELD RESERVED
Use whole TEST units for prices and budget. Use whole asset units for ownership quantities.";

fn number<T: FromStr>(value: &str, name: &str) -> Result<T, String> {
    value.parse().map_err(|_| format!("invalid {name}"))
}

fn run(args: &[String], input: &str) -> Result<String, String> {
    let [budget, now, age, cap_num, cap_den, metric_currency] = args else {
        return Err(USAGE.to_owned());
    };
    let budget: u64 = number(budget, "TEST budget")?;
    let window = ObservationWindow {
        now_unix_seconds: number(now, "valuation time")?,
        maximum_age_seconds: number(age, "maximum age")?,
    };
    let threshold = Rational::new(
        number(cap_num, "cap numerator")?,
        number(cap_den, "cap denominator")?,
    )
    .map_err(|error| format!("ownership cap: {error:?}"))?;
    if threshold.numerator() == 0 || threshold.numerator() > threshold.denominator() {
        return Err("ownership cap must be above zero and at most one".into());
    }
    let currency = Currency::new(
        metric_currency
            .as_bytes()
            .try_into()
            .map_err(|_| "invalid metric currency")?,
    )
    .map_err(|error| format!("metric currency: {error:?}"))?;

    let mut rows = input
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty());
    let (_, header) = rows.next().ok_or("missing SNAPSHOT header")?;
    let header_fields: Vec<_> = header.split_ascii_whitespace().take(5).collect();
    let ["SNAPSHOT", source, start, end] = header_fields.as_slice() else {
        return Err("expected SNAPSHOT SOURCE_ID PERIOD_START_UNIX PERIOD_END_UNIX".into());
    };
    if source.len() > 128
        || !source
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
    {
        return Err("invalid snapshot source identifier".into());
    }
    let period_start: u64 = number(start, "period start")?;
    let period_end: u64 = number(end, "period end")?;
    if period_start > period_end || period_end > window.now_unix_seconds {
        return Err(
            "reporting period must end on or after its start and at or before valuation".into(),
        );
    }

    let mut observations = Vec::new();
    let mut terms = BTreeMap::new();
    for (index, line) in rows {
        let row = index + 1;
        let fields: Vec<_> = line.split_ascii_whitespace().take(9).collect();
        let &[id, roic, fcf, time, price, issued, held, reserved] = fields.as_slice() else {
            return Err(format!("row {row}: expected eight fields"));
        };
        if !id.strip_prefix("TEST-").is_some_and(|suffix| {
            !suffix.is_empty()
                && suffix
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        }) || id.len() > 128
        {
            return Err(format!("row {row}: expected a fictitious TEST- asset ID"));
        }
        let as_of = number(time, "snapshot time")?;
        if as_of < period_end {
            return Err(format!("row {row}: snapshot predates reporting period end"));
        }
        let price: NonZeroU64 = number(price, "positive TEST price")?;
        let ownership = OwnershipSnapshot {
            as_of_unix_seconds: as_of,
            total_issued_units: Some(number(issued, "issued units")?),
            aggregated_held_units: Some(number(held, "aggregated held units")?),
            reserved_pending_units: Some(number(reserved, "reserved units")?),
        };
        let headroom = ownership_headroom(ownership, 0, threshold, window)
            .map_err(|error| format!("row {row}: {error:?}"))?;
        if terms
            .insert(id, (price, headroom.additional_units_before_proposal))
            .is_some()
        {
            return Err(format!("row {row}: duplicate asset ID"));
        }
        observations.push(Observation {
            asset_id: id.into(),
            roic_ppm: number(roic, "ROIC ppm")?,
            free_cash_flow_yield_ppm: number(fcf, "FCF yield ppm")?,
            metric_policy_id: METRIC_POLICY_ID.into(),
            currency,
            as_of_unix_seconds: as_of,
        });
    }

    let portfolio = research_weights(&observations, currency, window)
        .map_err(|error| format!("portfolio: {error:?}"))?;
    let mut remaining = u128::from(budget);
    let mut output = format!("Paper contribution: {budget} TEST\n");
    writeln!(
        output,
        "Source {source}, period {period_start}..{period_end}, valuation {}, metric currency {}",
        window.now_unix_seconds, metric_currency
    )
    .unwrap();
    writeln!(output, "Metric policy: {METRIC_POLICY_ID}").unwrap();
    let excluded = match portfolio {
        Portfolio::NoPortfolio {
            zero_score_assets,
            excluded,
        } => {
            output.push_str("No positive scores.\n");
            for id in zero_score_assets {
                writeln!(output, "{id} zero score").unwrap();
            }
            excluded
        }
        Portfolio::ResearchWeights {
            weights, excluded, ..
        } => {
            output.push_str("asset weight whole_units cost_TEST\n");
            for item in weights {
                let (price, headroom) = terms[item.asset_id.as_str()];
                let weight = item.weight;
                let target = u128::from(budget)
                    .checked_mul(weight.numerator())
                    .ok_or("TEST budget times weight numerator exceeds arithmetic range")?
                    / weight.denominator();
                let units = (target / u128::from(price.get())).min(headroom);
                let cost = units * u128::from(price.get());
                remaining -= cost;
                writeln!(
                    output,
                    "{} {}/{} {units} {cost}",
                    item.asset_id,
                    weight.numerator(),
                    weight.denominator()
                )
                .unwrap();
            }
            excluded
        }
    };
    for item in excluded {
        writeln!(output, "{} excluded ({:?})", item.asset_id, item.reason).unwrap();
    }
    writeln!(output, "Unallocated: {remaining} TEST").unwrap();
    Ok(output)
}

fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.as_slice() == ["--help"] {
        println!("{USAGE}");
        return std::process::ExitCode::SUCCESS;
    }
    if args.len() != 6 {
        eprintln!("{USAGE}");
        return std::process::ExitCode::FAILURE;
    }
    const MAX_BYTES: u64 = 1024 * 1024;
    let mut input = String::new();
    let result = match io::stdin().take(MAX_BYTES + 1).read_to_string(&mut input) {
        Ok(bytes) if bytes as u64 <= MAX_BYTES => run(&args, &input),
        Ok(_) => Err("snapshot exceeds 1 MiB".into()),
        Err(error) => Err(format!("could not read snapshot: {error}")),
    };
    match result {
        Ok(report) => {
            print!("{report}");
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
