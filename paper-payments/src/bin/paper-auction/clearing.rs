// SPDX-License-Identifier: GPL-3.0-only
use std::{cmp::Reverse, collections::BTreeMap};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Side {
    Buy,
    Sell,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Limit {
    pub side: Side,
    pub price: u64,
    pub units: u64,
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct Quote {
    pub price: u64,
    pub units: u128,
}

pub(super) fn clear(orders: &[Limit]) -> Option<Quote> {
    // Supply changes at the limit. A bid remains eligible through its limit.
    let mut changes = BTreeMap::<u64, (u128, u128)>::new();
    let mut demand = 0_u128;
    changes.insert(1, (0, 0));
    for order in orders {
        let units = u128::from(order.units);
        match order.side {
            Side::Sell => changes.entry(order.price).or_default().0 += units,
            Side::Buy => {
                demand += units;
                if let Some(next) = order.price.checked_add(1) {
                    changes.entry(next).or_default().1 += units;
                }
            }
        }
    }
    let mut supply = 0_u128;
    let mut best = None;
    for (price, (asks, expired_bids)) in changes {
        supply += asks;
        demand -= expired_bids;
        let units = supply.min(demand);
        if units > 0 {
            let rank = (units, Reverse(supply.abs_diff(demand)), Reverse(price));
            best = Some(best.map_or(rank, |previous| rank.max(previous)));
        }
    }
    best.map(|(units, _, Reverse(price))| Quote { price, units })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweep_matches_every_tick_for_small_books() {
        // Independent brute-force price search includes unquoted prices.
        let possibilities: Vec<_> = [Side::Buy, Side::Sell]
            .into_iter()
            .flat_map(|side| {
                (1..=4)
                    .flat_map(move |price| (1..=2).map(move |units| Limit { side, price, units }))
            })
            .collect();
        for a in &possibilities {
            for b in &possibilities {
                for c in &possibilities {
                    let orders = [*a, *b, *c];
                    let expected = (1..=4)
                        .filter_map(|price| {
                            let eligible = |o: &&Limit| match o.side {
                                Side::Buy => o.price >= price,
                                Side::Sell => o.price <= price,
                            };
                            let total = |side| {
                                orders
                                    .iter()
                                    .filter(eligible)
                                    .filter(|o| o.side == side)
                                    .map(|o| u128::from(o.units))
                                    .sum::<u128>()
                            };
                            let (buy, sell) = (total(Side::Buy), total(Side::Sell));
                            (buy.min(sell) > 0).then_some((
                                buy.min(sell),
                                Reverse(buy.abs_diff(sell)),
                                Reverse(price),
                            ))
                        })
                        .max()
                        .map(|(units, _, Reverse(price))| Quote { price, units });
                    assert_eq!(clear(&orders), expected);
                }
            }
        }
    }

    #[test]
    fn full_width_limits_and_volumes_do_not_wrap() {
        let buy = Limit {
            side: Side::Buy,
            units: u64::MAX,
            price: u64::MAX,
        };
        let sell = Limit {
            side: Side::Sell,
            ..buy
        };
        assert_eq!(
            clear(&[buy, buy, sell, sell]),
            Some(Quote {
                price: u64::MAX,
                units: 2 * u128::from(u64::MAX)
            })
        );
        assert_eq!(clear(&[buy]), None);
        assert_eq!(clear(&[]), None);
    }
}
