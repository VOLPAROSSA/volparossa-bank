// SPDX-License-Identifier: GPL-3.0-only
//! Bank → real ordered Core TEST execution; this example supplies order, NOT consensus.
use std::{
    error::Error,
    fs,
    os::unix::fs::DirBuilderExt as _,
    path::{Path, PathBuf},
};

use ed25519_dalek::SigningKey;
use rand_core::{OsRng, RngCore as _};
use volparossa_bank_paper_payments::{Authorization, Observation, OrderedIntent};
use volparossa_transaction::{
    Action, Balance, Block, BlockTime, Command, GenesisAccount, OrderedStore, Receipt,
};

fn id() -> [u8; 32] {
    std::array::from_fn(|_| OsRng.next_u32().to_le_bytes()[0])
}
fn block(height: u64, seconds: u64, bytes: Vec<u8>) -> Block {
    Block {
        height,
        block_id: id(),
        time: BlockTime { seconds, nanos: 0 },
        commands: vec![bytes],
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let root = PathBuf::from(
        args.next()
            .ok_or("usage: ordered_transfer NEW_ABSOLUTE_DIRECTORY")?,
    );
    if !root.is_absolute() || args.next().is_some() {
        return Err("provide exactly one new absolute directory".into());
    }
    let parent = root.parent().ok_or("private fixture parent required")?;
    if parent.canonicalize()? != parent {
        return Err("canonical fixture parent required".into());
    }
    fs::DirBuilder::new().mode(0o700).create(&root)?;
    let payer = SigningKey::generate(&mut OsRng);
    let recipient = SigningKey::generate(&mut OsRng);
    let payer_id = payer.verifying_key().to_bytes();
    let recipient_id = recipient.verifying_key().to_bytes();
    let mut core = OrderedStore::create(
        &root.join("core"),
        id(),
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
        BlockTime {
            seconds: 90,
            nanos: 0,
        },
    )?;
    let reservation_id = id();
    let reserve = OrderedIntent::prepare(
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
            nonce: id(),
        },
    )?;
    // The intent exposes only original bytes. This explicit TEST driver supplies
    // blocks and invokes Core; Bank has no convenience method that commits them.
    let first = block(1, 110, reserve.submission_bytes(&core)?.to_vec());
    core.stage(&first)?;
    assert_eq!(reserve.reconcile(&core)?, Observation::NotRecorded);
    core.commit(first.block_id)?.outcomes[0]
        .result
        .clone()
        .map_err(|_| "TEST reserve rejected")?;
    let commit = OrderedIntent::prepare(
        &root.join("commit"),
        &core,
        &payer,
        Command {
            operation_id: id(),
            payer: payer_id,
            action: Action::Commit { reservation_id },
        },
        Authorization {
            valid_from: 100,
            expires: 200,
            nonce: id(),
        },
    )?;
    let second = block(2, 120, commit.submission_bytes(&core)?.to_vec());
    core.stage(&second)?;
    assert_eq!(commit.reconcile(&core)?, Observation::NotRecorded);
    let receipt = core.commit(second.block_id)?.outcomes[0]
        .result
        .clone()
        .map_err(|_| "TEST commit rejected")?;
    drop(commit);
    drop(reserve);
    drop(core);
    drop(payer);
    drop(recipient);
    reopen_and_retry(&root, payer_id, recipient_id, receipt)?;
    println!(
        "PASS: ordered TEST intent retained -> staged absent -> committed -> reopened -> exact expired replay."
    );
    println!(
        "100 fictitious TEST units conserved: 30 payer / 70 recipient. No consensus or real settlement proved."
    );
    println!("Private fixture retained; ephemeral financial TEST keys were not saved.");
    Ok(())
}

fn reopen_and_retry(
    root: &Path,
    payer_id: [u8; 32],
    recipient_id: [u8; 32],
    receipt: Receipt,
) -> Result<(), Box<dyn Error>> {
    let mut core = OrderedStore::open(&root.join("core"))?;
    let recovered = OrderedIntent::open(&root.join("commit"), &core)?;
    assert_eq!(
        recovered.reconcile(&core)?,
        Observation::Recorded(receipt.clone())
    );
    let retry = block(3, 300, recovered.submission_bytes(&core)?.to_vec());
    core.stage(&retry)?;
    assert_eq!(core.commit(retry.block_id)?.outcomes[0].result, Ok(receipt));
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
    Ok(())
}
