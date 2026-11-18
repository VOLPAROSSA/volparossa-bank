// SPDX-License-Identifier: GPL-3.0-only
//! Actual Bank/core ordered TEST fixtures; supplied block order is NOT consensus.
use super::*;
use crate::Intent;
use rand_core::{OsRng, RngCore as _};
use std::{
    fs,
    os::unix::{
        fs::{PermissionsExt as _, symlink},
        process::ExitStatusExt as _,
    },
    process::{Child, Command as Process, Stdio},
    thread,
    time::{Duration, Instant},
};
use volparossa_transaction::{
    Action, Balance, Block, BlockTime, CommandFailure, GenesisAccount, Receipt, ReservationState,
    Store,
};

fn key(id: u8) -> SigningKey {
    SigningKey::from_bytes(&[id; 32])
}
fn owner(id: u8) -> [u8; 32] {
    key(id).verifying_key().to_bytes()
}
fn nonce() -> [u8; 32] {
    std::array::from_fn(|_| OsRng.next_u32().to_le_bytes()[0])
}
fn accounts() -> [GenesisAccount; 2] {
    [
        GenesisAccount {
            owner: owner(1),
            units: 100,
        },
        GenesisAccount {
            owner: owner(2),
            units: 0,
        },
    ]
}
fn store(path: &Path) -> OrderedStore {
    OrderedStore::create(
        path,
        [7; 32],
        &accounts(),
        BlockTime {
            seconds: 90,
            nanos: 0,
        },
    )
    .unwrap()
}
fn prepare(path: &Path, core: &OrderedStore, id: u8, action: Action) -> OrderedIntent {
    OrderedIntent::prepare(
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
            nonce: nonce(),
        },
    )
    .unwrap()
}
fn reserve(path: &Path, core: &OrderedStore) -> OrderedIntent {
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
fn block(height: u64, id: u8, seconds: u64, commands: Vec<Vec<u8>>) -> Block {
    Block {
        height,
        block_id: [id; 32],
        time: BlockTime { seconds, nanos: 0 },
        commands,
    }
}
fn committed(core: &mut OrderedStore, input: &Block) -> Receipt {
    let preview = core.stage(input).unwrap();
    let result = core.commit(input.block_id).unwrap();
    assert_eq!(result.app_hash, preview.app_hash);
    result.outcomes[0].result.clone().unwrap()
}
fn balances(core: &OrderedStore, payer: u64, reserved: u64, recipient: u64) {
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
fn staged_intent_remains_unrecorded_then_committed_receipts_survive_reopen_and_expired_replay() {
    let temp = tempfile::tempdir().unwrap();
    let core_path = temp.path().join("core");
    let reserve_path = temp.path().join("reserve");
    let mut core = store(&core_path);
    let intent = reserve(&reserve_path, &core);
    assert_eq!(intent.ledger_id(), core.ledger_id());
    assert_eq!(intent.command().operation_id, [1; 32]);
    let original = intent.submission_bytes(&core).unwrap().to_vec();
    let input = block(1, 1, 110, vec![original.clone()]);
    let staged = core.stage(&input).unwrap();
    assert!(staged.outcomes[0].result.is_ok());
    assert_eq!(intent.reconcile(&core).unwrap(), Observation::NotRecorded);
    assert_eq!(core.info().unwrap().height, 0);
    balances(&core, 100, 0, 0);
    let reserved = core.commit(input.block_id).unwrap().outcomes[0]
        .result
        .clone()
        .unwrap();
    drop(intent);
    drop(core);
    let mut core = OrderedStore::open(&core_path).unwrap();
    let intent = OrderedIntent::open(&reserve_path, &core).unwrap();
    assert_eq!(intent.submission_bytes(&core).unwrap(), original);
    assert_eq!(
        intent.reconcile(&core).unwrap(),
        Observation::Recorded(reserved.clone())
    );
    let commit_path = temp.path().join("commit");
    let commit = prepare(
        &commit_path,
        &core,
        2,
        Action::Commit {
            reservation_id: [1; 32],
        },
    );
    let signed = commit.submission_bytes(&core).unwrap().to_vec();
    let receipt = committed(&mut core, &block(2, 2, 120, vec![signed.clone()]));
    assert_eq!(receipt.state, ReservationState::Committed);
    drop(commit);
    drop(core);
    let mut core = OrderedStore::open(&core_path).unwrap();
    let commit = OrderedIntent::open(&commit_path, &core).unwrap();
    assert_eq!(commit.submission_bytes(&core).unwrap(), signed);
    assert_eq!(
        committed(&mut core, &block(3, 3, 300, vec![signed])),
        receipt
    );
    assert_eq!(
        commit.reconcile(&core).unwrap(),
        Observation::Recorded(receipt)
    );
    assert_eq!(
        intent.reconcile(&core).unwrap(),
        Observation::Recorded(reserved)
    );
    balances(&core, 30, 0, 70);
}

#[test]
fn expired_unknown_submission_is_exact_but_core_rejects_without_a_bank_success_receipt() {
    let temp = tempfile::tempdir().unwrap();
    let mut core = store(&temp.path().join("core"));
    let path = temp.path().join("intent");
    drop(reserve(&path, &core));
    let original_file = fs::read(path.join("intent.pb")).unwrap();
    let intent = OrderedIntent::open(&path, &core).unwrap();
    let original = intent.submission_bytes(&core).unwrap().to_vec();
    let input = block(1, 1, 200, vec![original.clone()]);
    assert_eq!(
        core.stage(&input).unwrap().outcomes[0].result,
        Err(CommandFailure::NotLive)
    );
    core.commit(input.block_id).unwrap();
    assert_eq!(intent.reconcile(&core).unwrap(), Observation::NotRecorded);
    assert_eq!(intent.submission_bytes(&core).unwrap(), original);
    assert_eq!(fs::read(path.join("intent.pb")).unwrap(), original_file);
    balances(&core, 100, 0, 0);
}

#[test]
fn ordered_cancel_is_committed_only_and_does_not_enable_later_commit_or_second_refund() {
    let temp = tempfile::tempdir().unwrap();
    let mut core = store(&temp.path().join("core"));
    let reserve = reserve(&temp.path().join("reserve"), &core);
    let bytes = reserve.submission_bytes(&core).unwrap().to_vec();
    committed(&mut core, &block(1, 1, 110, vec![bytes]));
    let cancel = prepare(
        &temp.path().join("cancel"),
        &core,
        2,
        Action::Cancel {
            reservation_id: [1; 32],
        },
    );
    let original = cancel.submission_bytes(&core).unwrap().to_vec();
    let input = block(2, 2, 120, vec![original.clone()]);
    core.stage(&input).unwrap();
    assert_eq!(cancel.reconcile(&core).unwrap(), Observation::NotRecorded);
    balances(&core, 30, 70, 0);
    let receipt = core.commit(input.block_id).unwrap().outcomes[0]
        .result
        .clone()
        .unwrap();
    assert_eq!(receipt.state, ReservationState::Cancelled);
    let commit = prepare(
        &temp.path().join("denied-commit"),
        &core,
        3,
        Action::Commit {
            reservation_id: [1; 32],
        },
    );
    let input = block(
        3,
        3,
        130,
        vec![commit.submission_bytes(&core).unwrap().to_vec()],
    );
    assert_eq!(
        core.stage(&input).unwrap().outcomes[0].result,
        Err(CommandFailure::State)
    );
    core.commit(input.block_id).unwrap();
    assert_eq!(commit.reconcile(&core).unwrap(), Observation::NotRecorded);
    assert_eq!(
        committed(&mut core, &block(4, 4, 300, vec![original])),
        receipt
    );
    assert_eq!(
        cancel.reconcile(&core).unwrap(),
        Observation::Recorded(receipt)
    );
    balances(&core, 100, 0, 0);
}

#[test]
fn domain_modes_and_enrollment_fail_before_intent_creation_or_submission() {
    let temp = tempfile::tempdir().unwrap();
    let core = store(&temp.path().join("ordered"));
    let wrong = OrderedStore::create(
        &temp.path().join("other"),
        [8; 32],
        &accounts(),
        BlockTime {
            seconds: 90,
            nanos: 0,
        },
    )
    .unwrap();
    let local = Store::create(&temp.path().join("local"), core.ledger_id(), &accounts()).unwrap();
    let path = temp.path().join("ordered-intent");
    let intent = reserve(&path, &core);
    assert!(intent.submission_bytes(&wrong).is_err());
    assert!(intent.reconcile(&wrong).is_err());
    assert!(OrderedIntent::open(&path, &wrong).is_err());
    assert!(Intent::open(&path, &local).is_err());
    assert!(
        local
            .inspect(intent.submission_bytes(&core).unwrap())
            .is_err()
    );
    let local_path = temp.path().join("local-intent");
    let old = Intent::prepare(
        &local_path,
        &local,
        &key(1),
        intent.command(),
        Authorization {
            valid_from: 100,
            expires: 200,
            nonce: nonce(),
        },
    )
    .unwrap();
    assert!(OrderedIntent::open(&local_path, &core).is_err());
    drop(old);
    let denied = temp.path().join("not-created");
    assert!(
        OrderedIntent::prepare(
            &denied,
            &core,
            &key(3),
            Command {
                operation_id: [3; 32],
                payer: owner(3),
                action: Action::Reserve {
                    recipient: owner(2),
                    units: 1
                },
            },
            Authorization {
                valid_from: 100,
                expires: 200,
                nonce: nonce()
            }
        )
        .is_err()
    );
    assert!(!denied.exists());
    balances(&core, 100, 0, 0);
}

#[test]
fn altered_noncanonical_or_cross_mode_retained_envelopes_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let core = store(&temp.path().join("core"));
    let path = temp.path().join("intent");
    drop(reserve(&path, &core));
    let file = path.join("intent.pb");
    let original = fs::read(&file).unwrap();
    for alteration in [
        "id",
        "ledger",
        "signature",
        "version",
        "unknown",
        "local-signed",
    ] {
        let mut retained = RetainedIntent::decode(original.as_slice()).unwrap();
        match alteration {
            "id" => retained.operation = vec![3; 32],
            "ledger" => retained.ledger = vec![8; 32],
            "signature" => {
                *retained.signed.last_mut().unwrap() ^= 1;
                retained.signed_sha256 = Sha256::digest(&retained.signed).to_vec();
            }
            "version" => retained.version = 1,
            "local-signed" => {
                retained.signed = SignedCommand::sign(
                    core.ledger_id(),
                    &key(1),
                    Command {
                        operation_id: [1; 32],
                        payer: owner(1),
                        action: Action::Reserve {
                            recipient: owner(2),
                            units: 70,
                        },
                    },
                    100,
                    200,
                    nonce(),
                )
                .unwrap()
                .encode();
                retained.signed_sha256 = Sha256::digest(&retained.signed).to_vec();
            }
            "unknown" => {}
            _ => unreachable!(),
        }
        let mut bytes = retained.encode_to_vec();
        if alteration == "unknown" {
            bytes.extend_from_slice(&[0xa0, 0x06, 1]);
        }
        fs::write(&file, bytes).unwrap();
        assert!(OrderedIntent::open(&path, &core).is_err());
        assert!(core.operation([1; 32]).unwrap().is_none());
        balances(&core, 100, 0, 0);
    }
}

#[test]
fn private_files_are_bounded_no_clobber_and_symlink_or_hardlink_inputs_are_refused() {
    let temp = tempfile::tempdir().unwrap();
    let core = store(&temp.path().join("core"));
    let path = temp.path().join("intent");
    let intent = reserve(&path, &core);
    let file = path.join("intent.pb");
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(
        OrderedIntent::prepare(
            &path,
            &core,
            &key(1),
            intent.command(),
            Authorization {
                valid_from: 100,
                expires: 200,
                nonce: nonce()
            }
        )
        .is_err()
    );
    let linked = temp.path().join("directory-link");
    symlink(&path, &linked).unwrap();
    assert!(OrderedIntent::open(&linked, &core).is_err());
    let alias = temp.path().join("hard-link");
    fs::hard_link(&file, &alias).unwrap();
    assert!(OrderedIntent::open(&path, &core).is_err());
    assert!(intent.submission_bytes(&core).is_err());
    fs::remove_file(alias).unwrap();
    drop(intent);
    fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(OrderedIntent::open(&path, &core).is_err());
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(&file, vec![0; crate::MAX_INTENT_BYTES + 1]).unwrap();
    assert!(OrderedIntent::open(&path, &core).is_err());
    fs::remove_file(&file).unwrap();
    symlink(temp.path().join("missing"), &file).unwrap();
    assert!(OrderedIntent::open(&path, &core).is_err());
    balances(&core, 100, 0, 0);
}

#[test]
fn held_intent_refuses_content_inode_and_directory_replacement_before_releasing_bytes() {
    for alteration in ["content", "inode", "directory"] {
        let temp = tempfile::tempdir().unwrap();
        let core = store(&temp.path().join("core"));
        let path = temp.path().join("intent");
        let intent = reserve(&path, &core);
        let file = path.join("intent.pb");
        let original = fs::read(&file).unwrap();
        match alteration {
            "content" => {
                let mut changed = original.clone();
                *changed.last_mut().unwrap() ^= 1;
                fs::write(&file, changed).unwrap();
            }
            "inode" => {
                let replacement = temp.path().join("new-file");
                fs::write(&replacement, &original).unwrap();
                fs::set_permissions(&replacement, fs::Permissions::from_mode(0o600)).unwrap();
                fs::rename(replacement, &file).unwrap();
            }
            "directory" => {
                fs::rename(&path, temp.path().join("old-directory")).unwrap();
                fs::create_dir(&path).unwrap();
                fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
                fs::write(&file, &original).unwrap();
                fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(intent.submission_bytes(&core).is_err());
        assert!(intent.reconcile(&core).is_err());
        balances(&core, 100, 0, 0);
    }
}

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        self.0.wait().expect("join owned synthetic crash child");
    }
}

#[test]
#[ignore = "only invoked by the owned ordered Bank crash regression"]
fn ordered_crash_child() {
    let Some(root) = std::env::var_os("VOLPAROSSA_BANK_ORDERED_CRASH_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let phase = std::env::var("VOLPAROSSA_BANK_ORDERED_CRASH_PHASE").unwrap();
    let mut core = OrderedStore::open(&root.join("core")).unwrap();
    let intent = OrderedIntent::open(&root.join("commit"), &core).unwrap();
    let bytes = intent.submission_bytes(&core).unwrap().to_vec();
    let input = block(2, 2, 120, vec![bytes]);
    core.stage(&input).unwrap();
    if phase == "after" {
        core.commit(input.block_id).unwrap();
    }
    fs::write(root.join("ready"), phase.as_bytes()).unwrap();
    thread::sleep(Duration::from_secs(30));
}

#[test]
fn killed_bank_process_reconciles_staged_and_committed_core_without_second_debit() {
    for phase in ["before", "after"] {
        let temp = tempfile::tempdir().unwrap();
        let core_path = temp.path().join("core");
        let mut core = store(&core_path);
        let reserve = reserve(&temp.path().join("reserve"), &core);
        let reserve_bytes = reserve.submission_bytes(&core).unwrap().to_vec();
        committed(&mut core, &block(1, 1, 110, vec![reserve_bytes]));
        drop(reserve);
        let commit_path = temp.path().join("commit");
        drop(prepare(
            &commit_path,
            &core,
            2,
            Action::Commit {
                reservation_id: [1; 32],
            },
        ));
        let original = fs::read(commit_path.join("intent.pb")).unwrap();
        drop(core);
        let mut child = OwnedChild(
            Process::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "ordered::tests::ordered_crash_child",
                    "--ignored",
                    "--nocapture",
                ])
                .env("VOLPAROSSA_BANK_ORDERED_CRASH_ROOT", temp.path())
                .env("VOLPAROSSA_BANK_ORDERED_CRASH_PHASE", phase)
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
        assert!(
            ready,
            "owned child reached selected stage/commit checkpoint"
        );
        child
            .0
            .kill()
            .expect("force actual owned Bank process crash");
        let killed = child.0.wait().expect("join killed Bank process");
        assert_eq!(
            killed.signal(),
            Some(9),
            "actual SIGKILL, not a graceful reopen"
        );
        drop(child); // Cached join is harmless; Drop still fences earlier failures.

        let mut core = OrderedStore::open(&core_path).unwrap();
        let intent = OrderedIntent::open(&commit_path, &core).unwrap();
        if phase == "before" {
            assert_eq!(intent.reconcile(&core).unwrap(), Observation::NotRecorded);
            balances(&core, 30, 70, 0);
            let original_bytes = intent.submission_bytes(&core).unwrap().to_vec();
            committed(&mut core, &block(2, 2, 120, vec![original_bytes]));
        } else {
            assert!(matches!(
                intent.reconcile(&core).unwrap(),
                Observation::Recorded(Receipt {
                    state: ReservationState::Committed,
                    ..
                })
            ));
        }
        let bytes = intent.submission_bytes(&core).unwrap().to_vec();
        let receipt = committed(&mut core, &block(3, 3, 300, vec![bytes]));
        assert_eq!(receipt.sequence, 2);
        assert_eq!(
            intent.reconcile(&core).unwrap(),
            Observation::Recorded(receipt)
        );
        assert_eq!(fs::read(commit_path.join("intent.pb")).unwrap(), original);
        balances(&core, 30, 0, 70);
    }
}
