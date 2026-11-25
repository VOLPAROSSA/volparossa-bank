// SPDX-License-Identifier: GPL-3.0-only
use super::{OrderBook, Side, clearing::Quote};
use std::collections::BTreeMap;

type Participant = [u8; 32];

// Each side receives the same crossing volume. At a given price each enrolled
// participant can take one unit per round, up to the size of its signed order.
pub(super) fn fills(orders: &OrderBook, quote: &Quote) -> BTreeMap<Participant, u64> {
    let mut fills = BTreeMap::new();
    for side in [Side::Buy, Side::Sell] {
        let mut eligible: Vec<_> = orders
            .iter()
            .filter(|(_, order)| {
                order.side == side
                    && match side {
                        Side::Buy => order.price >= quote.price,
                        Side::Sell => order.price <= quote.price,
                    }
            })
            .collect();
        eligible.sort_by(|(_, a), (_, b)| match side {
            Side::Buy => b.price.cmp(&a.price),
            Side::Sell => a.price.cmp(&b.price),
        });
        let mut remaining = quote.units;
        for tier in eligible.chunk_by(|(_, a), (_, b)| a.price == b.price) {
            if remaining == 0 {
                break;
            }
            let units: u128 = tier.iter().map(|(_, order)| u128::from(order.units)).sum();
            if units <= remaining {
                fills.extend(tier.iter().map(|(key, order)| (**key, order.units)));
                remaining -= units;
            } else {
                let mut requests: Vec<_> = tier
                    .iter()
                    .map(|(key, order)| (**key, order.units))
                    .collect();
                split_tier(&mut requests, remaining, &mut fills);
                break;
            }
        }
    }
    fills
}

fn split_tier(
    requests: &mut [(Participant, u64)],
    mut remaining: u128,
    fills: &mut BTreeMap<Participant, u64>,
) {
    requests.sort_unstable_by_key(|(_, units)| *units);
    for first in 0..requests.len() {
        let (key, units) = requests[first];
        let count = (requests.len() - first) as u128;
        let share = remaining / count;
        if u128::from(units) <= share {
            fills.insert(key, units);
            remaining -= u128::from(units);
        } else {
            // The smallest unfilled order exceeds the equal share. All remaining
            // orders can take that share, with at most one extra unit each.
            let share = u64::try_from(share).expect("share is below a u64 order");
            let extra = remaining % count;
            let unfilled = &mut requests[first..];
            unfilled.sort_unstable_by_key(|(key, _)| *key);
            for (index, &(key, _)) in unfilled.iter().enumerate() {
                let amount = share + u64::from((index as u128) < extra);
                if amount > 0 {
                    fills.insert(key, amount);
                }
            }
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Limit, clearing::clear};

    #[test]
    fn marginal_tier_matches_independent_one_unit_rounds() {
        for a in 1..=4 {
            for b in 1..=4 {
                for c in 1..=4 {
                    for available in 1..a + b + c {
                        let mut requests = [([1; 32], a), ([2; 32], b), ([3; 32], c)];
                        let mut expected = BTreeMap::new();
                        let mut left = available;
                        while left > 0 {
                            for &(key, units) in &requests {
                                let filled = expected.get(&key).copied().unwrap_or(0);
                                if left > 0 && filled < units {
                                    expected.insert(key, filled + 1);
                                    left -= 1;
                                }
                            }
                        }
                        requests.reverse();
                        let mut actual = BTreeMap::new();
                        split_tier(&mut requests, u128::from(available), &mut actual);
                        assert_eq!(actual, expected);
                    }
                }
            }
        }
    }

    #[test]
    fn price_priority_and_full_width_volume_hold_for_both_sides() {
        for scarce in [Side::Buy, Side::Sell] {
            let other = if scarce == Side::Buy {
                Side::Sell
            } else {
                Side::Buy
            };
            let better = if scarce == Side::Buy { 11 } else { 9 };
            let mut book = OrderBook::new();
            for (key, side, price, units) in [
                (1, scarce, better, u64::MAX),
                (2, scarce, 10, u64::MAX),
                (3, scarce, 10, u64::MAX),
                (4, other, 10, u64::MAX),
                (5, other, 10, u64::MAX),
            ] {
                book.insert([key; 32], Limit { side, price, units });
            }
            let quote = clear(book.values()).unwrap();
            assert_eq!(quote.units, 2 * u128::from(u64::MAX));
            assert_eq!(quote.price, 10);
            let actual = fills(&book, &quote);
            assert_eq!(actual[&[1; 32]], u64::MAX);
            assert_eq!(actual[&[2; 32]], u64::MAX.div_ceil(2));
            assert_eq!(actual[&[3; 32]], u64::MAX / 2);
            assert_eq!(actual[&[4; 32]], u64::MAX);
            assert_eq!(actual[&[5; 32]], u64::MAX);
        }
    }
}
