// SPDX-License-Identifier: GPL-3.0-only
//! Real core signed transactions and harmless owned crash children; TEST units only.

use std::{
    fs,
    os::unix::fs::{PermissionsExt as _, symlink},
    process::{Child, Command as Process, Stdio},
    thread,
    time::{Duration, Instant},
};

use super::*;
use rand_core::{OsRng, RngCore as _};
use volparossa_transaction::{Balance, Error as CoreError, GenesisAccount};

fn random_nonce() -> [u8; 32] {
    std::array::from_fn(|_| OsRng.next_u32().to_le_bytes()[0])
}

fn key(id: u8) -> SigningKey {
    SigningKey::from_bytes(&[id; 32])
}

fn owner(id: u8) -> [u8; 32] {
    key(id).verifying_key().to_bytes()
}

fn store(path: &Path) -> Store {
    Store::create(
        path,
        [9; 32],
        &[
            GenesisAccount {
                owner: owner(1),
                units: 100,
            },
            GenesisAccount {
                owner: owner(2),
                units: 0,
            },
        ],
    )
    .unwrap()
}

fn prepare(path: &Path, core: &Store, id: u8, action: Action) -> Intent {
    Intent::prepare(
        path,
        core,
        &key(1),
        Command {
            operation_id: [id; 32],
            payer: owner(1),
            action,
        },
        Authorization {
            valid_from: 100,
            expires: 200,
            nonce: random_nonce(),
        },
    )
    .unwrap()
}

fn reserve(path: &Path, core: &Store) -> Intent {
    prepare(
        path,
        core,
        1,
        Action::Reserve {
            recipient: owner(2),
            units: 70,
        },
    )
}

fn balances(core: &Store, payer: u64, reserved: u64, recipient: u64) {
    assert_eq!(
        core.status(owner(1)).unwrap(),
        Balance {
            available_units: payer,
            reserved_units: reserved
        }
    );
    assert_eq!(
        core.status(owner(2)).unwrap(),
        Balance {
            available_units: recipient,
            reserved_units: 0
        }
    );
}

#[test]
fn real_bank_to_core_flow_reopens_retries_and_keeps_receipts_historical() {
    let temp = tempfile::tempdir().unwrap();
    let core_path = temp.path().join("core");
    let reserve_path = temp.path().join("reserve");
    let mut core = store(&core_path);
    let intent = reserve(&reserve_path, &core);
    assert_eq!(intent.reconcile(&core).unwrap(), Observation::NotRecorded);
    balances(&core, 100, 0, 0);
    let reserved = intent.retry(&mut core, 110).unwrap();
    assert_eq!(reserved.state, ReservationState::Reserved);
    drop(intent);
    drop(core);

    let mut core = Store::open(&core_path).unwrap();
    let intent = Intent::open(&reserve_path, &core).unwrap();
    assert_eq!(intent.retry(&mut core, 300).unwrap(), reserved);
    balances(&core, 30, 70, 0);
    let commit_path = temp.path().join("commit");
    let commit = prepare(
        &commit_path,
        &core,
        2,
        Action::Commit {
            reservation_id: [1; 32],
        },
    );
    let receipt = commit.retry(&mut core, 120).unwrap();
    drop(commit);
    drop(core);

    let mut core = Store::open(&core_path).unwrap();
    let commit = Intent::open(&commit_path, &core).unwrap();
    assert_eq!(
        commit.reconcile(&core).unwrap(),
        Observation::Recorded(receipt.clone())
    );
    assert_eq!(commit.retry(&mut core, 300).unwrap(), receipt);
    assert_eq!(
        intent.reconcile(&core).unwrap(),
        Observation::Recorded(reserved)
    );
    balances(&core, 30, 0, 70);

    let cancel = prepare(
        &temp.path().join("cancel"),
        &core,
        3,
        Action::Cancel {
            reservation_id: [1; 32],
        },
    );
    assert!(matches!(
        cancel.retry(&mut core, 130),
        Err(Error::Core(CoreError::State))
    ));
    assert_eq!(cancel.reconcile(&core).unwrap(), Observation::NotRecorded);
    balances(&core, 30, 0, 70);
}

#[test]
fn unknown_expired_intent_stays_unrecorded_without_implicit_resigning() {
    let temp = tempfile::tempdir().unwrap();
    let mut core = store(&temp.path().join("core"));
    let path = temp.path().join("intent");
    drop(reserve(&path, &core));
    let original = fs::read(path.join("intent.pb")).unwrap();
    let intent = Intent::open(&path, &core).unwrap();
    assert!(matches!(
        intent.retry(&mut core, 200),
        Err(Error::Core(CoreError::NotLive))
    ));
    assert_eq!(intent.reconcile(&core).unwrap(), Observation::NotRecorded);
    assert_eq!(fs::read(path.join("intent.pb")).unwrap(), original);
    balances(&core, 100, 0, 0);
}

#[test]
fn altered_descriptor_and_signed_bytes_are_rejected_before_any_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let core = store(&temp.path().join("core"));
    let path = temp.path().join("intent");
    drop(reserve(&path, &core));
    let file = path.join("intent.pb");
    let original = fs::read(&file).unwrap();
    for alteration in ["id", "ledger", "signed", "version", "unknown-field"] {
        let mut retained = RetainedIntent::decode(original.as_slice()).unwrap();
        match alteration {
            "id" => retained.operation = vec![2; 32],
            "ledger" => retained.ledger = vec![8; 32],
            "signed" => {
                *retained.signed.last_mut().unwrap() ^= 1;
                retained.signed_sha256 = Sha256::digest(&retained.signed).to_vec();
            }
            "version" => retained.version = 2,
            "unknown-field" => {}
            _ => unreachable!(),
        }
        let mut bytes = retained.encode_to_vec();
        if alteration == "unknown-field" {
            bytes.extend_from_slice(&[0xa0, 0x06, 1]);
        }
        fs::write(&file, &bytes).unwrap();
        assert!(Intent::open(&path, &core).is_err());
        balances(&core, 100, 0, 0);
        assert!(core.operation([1; 32]).unwrap().is_none());
    }
}

#[test]
fn retained_files_are_private_bounded_and_never_adopted_or_symlinked() {
    let temp = tempfile::tempdir().unwrap();
    let core = store(&temp.path().join("core"));
    let path = temp.path().join("intent");
    drop(reserve(&path, &core));
    let file = path.join("intent.pb");
    let original = fs::read(&file).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let command = Command {
        operation_id: [3; 32],
        payer: owner(1),
        action: Action::Reserve {
            recipient: owner(2),
            units: 1,
        },
    };
    assert!(
        Intent::prepare(
            &path,
            &core,
            &key(1),
            command,
            Authorization {
                valid_from: 100,
                expires: 200,
                nonce: random_nonce()
            }
        )
        .is_err()
    );
    assert_eq!(fs::read(&file).unwrap(), original);
    fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(Intent::open(&path, &core).is_err());
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    let alias = temp.path().join("alias");
    symlink(&path, &alias).unwrap();
    assert!(Intent::open(&alias, &core).is_err());
    let saved = path.join("original.pb");
    fs::rename(&file, &saved).unwrap();
    symlink(&saved, &file).unwrap();
    assert!(Intent::open(&path, &core).is_err());
    fs::remove_file(&file).unwrap();
    fs::rename(&saved, &file).unwrap();
    fs::hard_link(&file, path.join("hard-link")).unwrap();
    assert!(Intent::open(&path, &core).is_err());
    fs::remove_file(path.join("hard-link")).unwrap();
    fs::write(&file, vec![0; MAX_INTENT_BYTES + 1]).unwrap();
    assert!(Intent::open(&path, &core).is_err());
    fs::write(&file, []).unwrap();
    assert!(Intent::open(&path, &core).is_err());
    balances(&core, 100, 0, 0);
}

#[test]
fn different_accepted_command_with_same_id_is_not_this_intents_receipt() {
    let temp = tempfile::tempdir().unwrap();
    let mut core = store(&temp.path().join("core"));
    let intended = reserve(&temp.path().join("intended"), &core);
    let other = SignedCommand::sign(
        core.ledger_id(),
        &key(1),
        Command {
            operation_id: [1; 32],
            payer: owner(1),
            action: Action::Reserve {
                recipient: owner(2),
                units: 20,
            },
        },
        100,
        200,
        random_nonce(),
    )
    .unwrap()
    .encode();
    core.apply(&other, 110).unwrap();
    assert!(matches!(intended.reconcile(&core), Err(Error::Binding)));
    assert!(matches!(
        intended.retry(&mut core, 120),
        Err(Error::Core(CoreError::Conflict))
    ));
    balances(&core, 80, 20, 0);
}

#[test]
fn only_explicit_owner_authorization_creates_a_new_cancel_and_returns_reserved_units() {
    let temp = tempfile::tempdir().unwrap();
    let mut core = store(&temp.path().join("core"));
    let intent = reserve(&temp.path().join("reserve"), &core);
    intent.retry(&mut core, 110).unwrap();
    let unauthorized = temp.path().join("unauthorized");
    assert!(
        Intent::prepare(
            &unauthorized,
            &core,
            &key(3),
            Command {
                operation_id: [5; 32],
                payer: owner(3),
                action: Action::Cancel {
                    reservation_id: [1; 32]
                }
            },
            Authorization {
                valid_from: 100,
                expires: 200,
                nonce: random_nonce()
            },
        )
        .is_err()
    );
    assert!(!unauthorized.exists());
    balances(&core, 30, 70, 0);
    let cancel = prepare(
        &temp.path().join("cancel"),
        &core,
        2,
        Action::Cancel {
            reservation_id: [1; 32],
        },
    );
    let cancelled = cancel.retry(&mut core, 120).unwrap();
    assert_eq!(cancelled.state, ReservationState::Cancelled);
    assert_eq!(cancel.retry(&mut core, 300).unwrap(), cancelled);
    balances(&core, 100, 0, 0);
}

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
#[ignore = "only invoked by the owned paper-payment crash regression"]
fn crash_child() {
    let Some(root) = std::env::var_os("VOLPAROSSA_BANK_TEST_CRASH_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let phase = std::env::var("VOLPAROSSA_BANK_TEST_CRASH_PHASE").unwrap();
    let mut core = Store::open(&root.join("core")).unwrap();
    let intent = Intent::open(&root.join("commit"), &core).unwrap();
    if phase == "after" {
        intent.retry(&mut core, 120).unwrap();
    }
    // The parent receives no receipt. Only an owned synthetic checkpoint is exposed.
    fs::write(root.join("ready"), phase.as_bytes()).unwrap();
    thread::sleep(Duration::from_secs(30));
}

#[test]
fn killed_application_reconciles_before_and_after_core_commit_without_double_credit() {
    for phase in ["before", "after"] {
        let temp = tempfile::tempdir().unwrap();
        let core_path = temp.path().join("core");
        let commit_path = temp.path().join("commit");
        let mut core = store(&core_path);
        reserve(&temp.path().join("reserve"), &core)
            .retry(&mut core, 110)
            .unwrap();
        drop(prepare(
            &commit_path,
            &core,
            2,
            Action::Commit {
                reservation_id: [1; 32],
            },
        ));
        drop(core);
        let original = fs::read(commit_path.join("intent.pb")).unwrap();
        let mut child = OwnedChild(
            Process::new(std::env::current_exe().unwrap())
                .args(["--exact", "tests::crash_child", "--ignored", "--nocapture"])
                .env("VOLPAROSSA_BANK_TEST_CRASH_ROOT", temp.path())
                .env("VOLPAROSSA_BANK_TEST_CRASH_PHASE", phase)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while !temp.path().join("ready").exists()
            && Instant::now() < deadline
            && child.0.try_wait().unwrap().is_none()
        {
            thread::sleep(Duration::from_millis(10));
        }
        let ready = temp.path().join("ready").exists();
        drop(child); // kill and join before inspecting the original core store.
        assert!(
            ready,
            "actual child reached the selected application checkpoint"
        );
        let mut core = Store::open(&core_path).unwrap();
        let intent = Intent::open(&commit_path, &core).unwrap();
        if phase == "before" {
            assert_eq!(intent.reconcile(&core).unwrap(), Observation::NotRecorded);
            balances(&core, 30, 70, 0);
            intent.retry(&mut core, 120).unwrap();
        } else {
            assert!(matches!(
                intent.reconcile(&core).unwrap(),
                Observation::Recorded(Receipt {
                    state: ReservationState::Committed,
                    ..
                })
            ));
        }
        let receipt = intent.retry(&mut core, 300).unwrap();
        assert_eq!(receipt.sequence, 2);
        assert_eq!(
            intent.reconcile(&core).unwrap(),
            Observation::Recorded(receipt)
        );
        balances(&core, 30, 0, 70);
        assert_eq!(fs::read(commit_path.join("intent.pb")).unwrap(), original);
    }
}
