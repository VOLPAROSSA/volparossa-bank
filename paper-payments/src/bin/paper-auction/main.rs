// SPDX-License-Identifier: GPL-3.0-only
//! Replay a single fictional uniform-price auction with separately enrolled TEST signers.
mod clearing;

use clearing::{Limit, Side};
use ed25519_dalek::{Signature, Signer as _, SigningKey, VerifyingKey};
use sha2::{Digest as _, Sha256};
use std::{
    collections::BTreeSet,
    env,
    error::Error,
    fmt::Write as _,
    fs::File,
    io::{self, Read},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const MAX_MEMBERS: usize = 1024;
const DOMAIN: &[u8] = b"VOLPAROSSA-BANK/TEST-SHARE/TEST/auction/v1\0";

fn limited(reader: impl Read, max: usize) -> Result<String> {
    let mut data = String::new();
    reader
        .take(u64::try_from(max + 1)?)
        .read_to_string(&mut data)?;
    if data.len() > max || !data.is_ascii() {
        return Err("oversize or non-ASCII input".into());
    }
    Ok(data)
}

fn decode<const N: usize>(encoded: &str) -> Result<[u8; N]> {
    if encoded.len() != N * 2
        || !encoded
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err("expected lowercase hexadecimal bytes".into());
    }
    let mut bytes = [0; N];
    let (pairs, _) = encoded.as_bytes().as_chunks::<2>();
    for (out, pair) in bytes.iter_mut().zip(pairs) {
        *out = u8::from_str_radix(std::str::from_utf8(pair)?, 16)?;
    }
    Ok(bytes)
}

fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(2 * bytes.len());
    for byte in bytes {
        write!(text, "{byte:02x}").unwrap();
    }
    text
}

fn roster_digest(batch: &[u8; 32], members: &BTreeSet<[u8; 32]>) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"VOLPAROSSA-BANK/TEST-SHARE/TEST/roster/v1\0");
    hash.update(batch);
    for member in members {
        hash.update(member);
    }
    hash.finalize().into()
}

fn preimage(roster: &[u8; 32], key: &[u8; 32], order: Limit) -> Vec<u8> {
    let mut bytes = Vec::from(DOMAIN);
    bytes.extend(roster);
    bytes.extend(key);
    bytes.push(match order.side {
        Side::Buy => 0,
        Side::Sell => 1,
    });
    bytes.extend(order.units.to_be_bytes());
    bytes.extend(order.price.to_be_bytes());
    bytes
}

fn verify(roster: &str, book: &str) -> Result<Option<clearing::Quote>> {
    let mut lines = roster.lines();
    let batch = decode(lines.next().ok_or("missing batch identifier")?)?;
    if batch == [0; 32] {
        return Err("missing batch identifier".into());
    }
    let mut members = BTreeSet::new();
    for line in lines {
        let key = decode(line)?;
        if members.len() >= MAX_MEMBERS
            || !members.insert(key)
            || VerifyingKey::from_bytes(&key)?.is_weak()
        {
            return Err("invalid auction membership".into());
        }
    }
    if members.is_empty() {
        return Err("empty membership".into());
    }
    let roster = roster_digest(&batch, &members);
    let mut orders = Vec::new();
    for line in book.lines() {
        let fields: Vec<_> = line.split_ascii_whitespace().take(6).collect();
        let &[key, side, units, price, signature] = fields.as_slice() else {
            return Err("expected key, side, units, limit and signature".into());
        };
        let key = decode(key)?;
        if !members.remove(&key) {
            return Err("unknown or repeated auction participant".into());
        }
        let side = match side {
            "buy" => Side::Buy,
            "sell" => Side::Sell,
            _ => return Err("unsupported side".into()),
        };
        let order = Limit {
            side,
            price: price.parse()?,
            units: units.parse()?,
        };
        if order.units == 0 || order.price == 0 {
            return Err("positive units and price required".into());
        }
        let signature = Signature::from_bytes(&decode(signature)?);
        VerifyingKey::from_bytes(&key)?
            .verify_strict(&preimage(&roster, &key, order), &signature)?;
        orders.push(order);
    }
    Ok(clearing::clear(&orders))
}

// Public deterministic TEST keys are confined to this printed synthetic sample.
fn sample() -> (String, String) {
    let batch = [42; 32];
    let mut roster = hex(&batch) + "\n";
    let mut orders = String::new();
    let orders_to_sign = [
        (1, Side::Buy, 7, 12),
        (2, Side::Buy, 3, 9),
        (3, Side::Sell, 5, 8),
        (4, Side::Sell, 4, 10),
    ]
    .map(|(seed, side, units, price)| {
        (
            SigningKey::from_bytes(&[seed; 32]),
            Limit { side, price, units },
        )
    });
    let keys = orders_to_sign
        .iter()
        .map(|(key, _)| key.verifying_key().to_bytes())
        .collect();
    let digest = roster_digest(&batch, &keys);
    for (key, order) in orders_to_sign {
        let public = key.verifying_key().to_bytes();
        let name = hex(&public);
        let signature = key.sign(&preimage(&digest, &public, order));
        writeln!(roster, "{name}").unwrap();
        let Limit { side, units, price } = order;
        let side = if side == Side::Buy { "buy" } else { "sell" };
        writeln!(
            orders,
            "{name} {side} {units} {price} {}",
            hex(&signature.to_bytes())
        )
        .unwrap();
    }
    (roster, orders)
}

fn main() -> Result<()> {
    let args: Vec<_> = env::args().skip(1).collect();
    match args.as_slice() {
        [cmd] if cmd == "sample-roster" => print!("{}", sample().0),
        [cmd] if cmd == "sample-orders" => print!("{}", sample().1),
        [cmd, roster] if cmd == "verify" => {
            let members = limited(File::open(roster)?, (MAX_MEMBERS + 1) * 66)?;
            let book = limited(io::stdin().lock(), MAX_MEMBERS * 245)?;
            match verify(&members, &book)? {
                Some(quote) => println!(
                    "TEST-SHARE: {} units at {} TEST per unit",
                    quote.units, quote.price
                ),
                None => println!("TEST-SHARE: no crossing orders"),
            }
        }
        _ => {
            return Err(
                "usage: paper-auction sample-roster | sample-orders | verify ROSTER < ORDERS"
                    .into(),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_replay_is_order_independent_and_bound_to_the_batch() {
        let (roster, orders) = sample();
        let expected = Some(clearing::Quote {
            price: 10,
            units: 7,
        });
        assert_eq!(verify(&roster, &orders).unwrap(), expected);
        let reversed = orders.lines().rev().collect::<Vec<_>>().join("\n");
        assert_eq!(verify(&roster, &reversed).unwrap(), expected);
        let changed_batch = roster.replacen(&hex(&[42; 32]), &hex(&[43; 32]), 1);
        for (members, book) in [
            (changed_batch, orders.clone()),
            (roster.clone(), orders.replace("buy 7 12", "buy 7 13")),
            (roster.clone(), orders.replace("buy 7 12", "sell 7 12")),
            (roster.clone(), orders.replace("buy 7 12", "buy 6 12")),
            (roster.clone(), orders.clone() + &orders),
            (
                roster.lines().take(4).collect::<Vec<_>>().join("\n"),
                orders.clone(),
            ),
        ] {
            assert!(verify(&members, &book).is_err());
        }
        // Removing a non-submitting member still changes the signed roster.
        let subset = roster.lines().take(4).collect::<Vec<_>>().join("\n");
        let first = orders.lines().next().unwrap();
        assert!(verify(&subset, first).is_err());
        assert_eq!(verify(&roster, "").unwrap(), None);
    }
}
