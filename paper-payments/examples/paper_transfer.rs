// SPDX-License-Identifier: GPL-3.0-only
//! Explicit offline Bank intent → real core TEST-unit transfer, without stored keys.

use std::{error::Error, fs, os::unix::fs::DirBuilderExt as _, path::PathBuf};

use ed25519_dalek::SigningKey;
use rand_core::{OsRng, RngCore as _};
use volparossa_bank_paper_payments::{Authorization, Intent, Observation};
use volparossa_transaction::{Action, Balance, Command, GenesisAccount, Store, TEST_UNIT};

fn random_id() -> [u8; 32] {
    std::array::from_fn(|_| OsRng.next_u32().to_le_bytes()[0])
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let root = PathBuf::from(
        args.next()
            .ok_or("usage: paper_transfer NEW_ABSOLUTE_DIRECTORY")?,
    );
    if !root.is_absolute() || args.next().is_some() {
        return Err("provide exactly one new absolute directory".into());
    }
    fs::DirBuilder::new().mode(0o700).create(&root)?;
    let payer = SigningKey::generate(&mut OsRng);
    let recipient = SigningKey::generate(&mut OsRng);
    let payer_id = payer.verifying_key().to_bytes();
    let recipient_id = recipient.verifying_key().to_bytes();
    let mut core = Store::create(
        &root.join("core"),
        random_id(),
        &[
            GenesisAccount {
                owner: payer_id,
                units: 100,
            },
            GenesisAccount {
                owner: recipient_id,
                units: 0,
            },
        ],
    )?;
    let reservation_id = random_id();
    let reserve = Intent::prepare(
        &root.join("reserve"),
        &core,
        &payer,
        Command {
            operation_id: reservation_id,
            payer: payer_id,
            action: Action::Reserve {
                recipient: recipient_id,
                units: 70,
            },
        },
        Authorization {
            valid_from: 100,
            expires: 200,
            nonce: random_id(),
        },
    )?;
    assert_eq!(reserve.reconcile(&core)?, Observation::NotRecorded);
    reserve.retry(&mut core, 110)?;
    let commit = Intent::prepare(
        &root.join("commit"),
        &core,
        &payer,
        Command {
            operation_id: random_id(),
            payer: payer_id,
            action: Action::Commit { reservation_id },
        },
        Authorization {
            valid_from: 100,
            expires: 200,
            nonce: random_id(),
        },
    )?;
    let receipt = commit.retry(&mut core, 120)?;
    // The application deliberately keeps no second receipt/saldo database.
    drop(commit);
    drop(reserve);
    drop(core);
    drop(payer);
    drop(recipient);

    let mut core = Store::open(&root.join("core"))?;
    let recovered = Intent::open(&root.join("commit"), &core)?;
    assert_eq!(
        recovered.reconcile(&core)?,
        Observation::Recorded(receipt.clone())
    );
    assert_eq!(recovered.retry(&mut core, 300)?, receipt);
    assert_eq!(
        core.status(payer_id)?,
        Balance {
            available_units: 30,
            reserved_units: 0
        }
    );
    assert_eq!(
        core.status(recipient_id)?,
        Balance {
            available_units: 70,
            reserved_units: 0
        }
    );
    println!(
        "PASS: Bank intent -> core reserve/commit -> reopen/reconcile/exact expired retry; {TEST_UNIT}"
    );
    println!("Only fictitious units: 30 payer / 70 recipient. No network or real settlement.");
    println!(
        "Private fixture retained at {}; no private keys saved.",
        root.display()
    );
    Ok(())
}
