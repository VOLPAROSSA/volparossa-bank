// SPDX-License-Identifier: GPL-3.0-only
//! Prepare or reopen a private Bank TEST intent for an external four-validator trial.

use std::{
    fs::{self, File},
    io::{self, Read, Write},
    os::unix::fs::MetadataExt as _,
    path::Path,
    process::ExitCode,
};

use ed25519_dalek::SigningKey;
use rand_core::{OsRng, RngCore as _};
use rustix::fs::{CWD, RenameFlags, renameat_with};
use volparossa_bank_paper_payments::{Authorization, OrderedIntent};
use volparossa_transaction::{Action, Command, OrderedStore};
use volparossa_transaction_abci::{
    Application, Genesis, consensus_params,
    proto::{google::protobuf::Timestamp, tendermint::abci},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn identifier(value: &str) -> Result<[u8; 32]> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("invalid identifier".into());
    }
    let mut id = [0; 32];
    for (out, pair) in id.iter_mut().zip(value.as_bytes().chunks_exact(2)) {
        *out = u8::from_str_radix(std::str::from_utf8(pair)?, 16)?;
    }
    Ok(id)
}

fn context(path: &Path, ledger: [u8; 32]) -> Result<OrderedStore> {
    let core = OrderedStore::open(path)?;
    if core.ledger_id() != ledger || core.info()?.height != 0 {
        return Err("unexpected owner genesis".into());
    }
    Ok(core)
}

fn create_genesis(path: &Path, bytes: Vec<u8>) -> Result<()> {
    let genesis = Genesis::from_json(&bytes)?;
    let request = abci::RequestInitChain {
        time: Some(Timestamp {
            seconds: genesis.seconds,
            nanos: genesis.nanos,
        }),
        chain_id: genesis.chain_id.clone(),
        consensus_params: Some(consensus_params()),
        validators: genesis.validator_updates()?,
        app_state_bytes: bytes,
        initial_height: 1,
    };
    let mut app = Application::open(path, genesis)?;
    let response = app.handle(abci::Request {
        value: Some(abci::request::Value::InitChain(request)),
    })?;
    if !matches!(response.value, Some(abci::response::Value::InitChain(_))) {
        return Err("genesis rejected".into());
    }
    Ok(())
}

fn initialize(owner: &Path, ledger: [u8; 32], input: &mut dyn Read) -> Result<()> {
    if fs::read_dir(owner)?.next().is_some() {
        return Err("fresh owner directory required".into());
    }
    let mut bytes = Vec::new();
    input.take(16_385).read_to_end(&mut bytes)?;
    let staging = tempfile::tempdir_in(owner)?;
    let private = staging.path().join("core");
    create_genesis(&private, bytes)?;
    context(&private, ledger)?;
    renameat_with(CWD, &private, CWD, owner.join("core"), RenameFlags::NOREPLACE)?;
    File::open(owner)?.sync_all()?;
    Ok(())
}

fn prepare(
    core: &OrderedStore,
    path: &Path,
    operation: [u8; 32],
    args: &[String],
    verb: &str,
    input: &mut dyn Read,
) -> Result<()> {
    let [start, end, terms @ ..] = args else {
        return Err("authorization window required".into());
    };
    let action = match (verb, terms) {
        ("reserve", [recipient, units]) => Action::Reserve {
            recipient: identifier(recipient)?,
            units: units.parse()?,
        },
        ("commit", [reservation]) => Action::Commit {
            reservation_id: identifier(reservation)?,
        },
        _ => return Err("invalid TEST operation".into()),
    };
    let mut seed = [0; 32];
    input.read_exact(&mut seed)?;
    if input.read(&mut [0; 1])? != 0 {
        return Err("invalid seed length".into());
    }
    let signer = SigningKey::from_bytes(&seed);
    let mut nonce = [0; 32];
    OsRng
        .try_fill_bytes(&mut nonce)
        .map_err(|_| "random nonce unavailable")?;
    OrderedIntent::prepare(
        path,
        core,
        &signer,
        Command {
            operation_id: operation,
            payer: signer.verifying_key().to_bytes(),
            action,
        },
        Authorization {
            valid_from: start.parse()?,
            expires: end.parse()?,
            nonce,
        },
    )?;
    Ok(())
}

fn run(args: &[String], input: &mut dyn Read, output: &mut dyn Write) -> Result<()> {
    let [verb, owner, ledger, args @ ..] = args else {
        return Err("operation, owner directory and ledger required".into());
    };
    let owner = Path::new(owner);
    let meta = owner.symlink_metadata()?;
    if !meta.is_dir()
        || owner.canonicalize()? != owner
        || meta.mode() & 0o7777 != 0o700
        || meta.uid() != rustix::process::geteuid().as_raw()
    {
        return Err("private owner directory required".into());
    }
    let ledger = identifier(ledger)?;
    if verb == "init" && args.is_empty() {
        initialize(owner, ledger, input)?;
        output.write_all(b"ready\n")?;
        return Ok(());
    }
    let [operation, args @ ..] = args else {
        return Err("operation identifier required".into());
    };
    let id = identifier(operation)?;
    let path = owner.join(operation);
    let core = context(&owner.join("core"), ledger)?;
    match verb.as_str() {
        "replay" if args.is_empty() => {}
        "reserve" | "commit" => prepare(&core, &path, id, args, verb, input)?,
        _ => return Err("unknown request".into()),
    }
    let retained = OrderedIntent::open(&path, &core)?;
    if retained.command().operation_id != id {
        return Err("operation binding mismatch".into());
    }
    output.write_all(retained.submission_bytes(&core)?)?;
    Ok(())
}

fn main() -> ExitCode {
    let args = std::env::args_os()
        .skip(1)
        .map(std::ffi::OsString::into_string)
        .collect::<std::result::Result<Vec<_>, _>>();
    if let Ok(args) = args
        && run(&args, &mut io::stdin().lock(), &mut io::stdout().lock()).is_ok()
    {
        ExitCode::SUCCESS
    } else {
        eprintln!("Bank TEST authorizer rejected the request");
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fmt::Write as _, os::unix::fs::PermissionsExt as _};

    // Public keys for the six distinct synthetic Ed25519 seeds [1; 32] through [6; 32].
    const GENESIS: &[u8] = br#"{
        "version": 1, "chain_id": "volparossa-test-bank-owner", "seconds": 90, "nanos": 0,
        "validators": [
            "ed4928c628d1c2c6eae90338905995612959273a5c63f93636c14614ac8737d1",
            "ca93ac1705187071d67b83c7ff0efe8108e8ec4530575d7726879333dbdabe7c",
            "6e7a1cdd29b0b78fd13af4c5598feff4ef2a97166e3ca6f2e4fbfccd80505bf1",
            "8a875fff1eb38451577acd5afee405456568dd7c89e090863a0557bc7af49f17"
        ],
        "accounts": [
            {"public_key": "8a88e3dd7409f195fd52db2d3cba5d72ca6709bf1d94121bf3748801b40f6f5c", "units": 100},
            {"public_key": "8139770ea87d175f56a35466c34c7ecccb8d8a91b4ee37a25df60f5b8fc9b394", "units": 0}
        ]
    }"#;

    fn hex(bytes: [u8; 32]) -> String {
        let mut value = String::with_capacity(64);
        for byte in bytes {
            write!(value, "{byte:02x}").unwrap();
        }
        value
    }

    fn invoke(
        owner: &Path,
        ledger: &str,
        verb: &str,
        terms: &[&str],
        mut input: &[u8],
    ) -> Result<Vec<u8>> {
        let mut args = vec![
            verb.to_owned(),
            owner.to_str().unwrap().to_owned(),
            ledger.to_owned(),
        ];
        args.extend(terms.iter().map(|term| (*term).to_owned()));
        let mut output = vec![];
        run(&args, &mut input, &mut output)?;
        Ok(output)
    }

    #[test]
    fn verified_genesis_and_retained_authorizations_never_advance_owner_state() {
        let authority = tempfile::tempdir().unwrap();
        let store = authority.path().join("validator-genesis");
        create_genesis(&store, GENESIS.to_vec()).unwrap();
        let ledger = hex(OrderedStore::open(&store).unwrap().ledger_id());
        let owner = tempfile::tempdir().unwrap();
        let root = owner.path();
        fs::set_permissions(root, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(invoke(root, &"00".repeat(32), "init", &[], GENESIS).is_err());
        assert_eq!(fs::read_dir(root).unwrap().count(), 0);
        assert_eq!(
            invoke(root, &ledger, "init", &[], GENESIS).unwrap(),
            b"ready\n"
        );

        let operation = "ab".repeat(32);
        let recipient = hex(SigningKey::from_bytes(&[2; 32]).verifying_key().to_bytes());
        let reserve = [operation.as_str(), "100", "200", recipient.as_str(), "70"];
        assert!(invoke(root, &ledger, "reserve", &reserve, &[1; 33]).is_err());
        assert!(invoke(root, &ledger, "reserve", &reserve, &[7; 32]).is_err());
        assert!(!root.join(&operation).exists());
        let original = invoke(root, &ledger, "reserve", &reserve, &[1; 32]).unwrap();
        assert!(invoke(root, &ledger, "reserve", &reserve, &[1; 32]).is_err());
        assert_eq!(
            invoke(root, &ledger, "replay", &[&operation], &[]).unwrap(),
            original
        );

        let commit_id = "cd".repeat(32);
        let commit = [commit_id.as_str(), "100", "200", operation.as_str()];
        let signed = invoke(root, &ledger, "commit", &commit, &[1; 32]).unwrap();
        assert_eq!(
            invoke(root, &ledger, "replay", &[&commit_id], &[]).unwrap(),
            signed
        );
        let core = context(&root.join("core"), identifier(&ledger).unwrap()).unwrap();
        assert_eq!(
            core.inspect(&original).unwrap().action,
            Action::Reserve {
                recipient: identifier(&recipient).unwrap(),
                units: 70,
            }
        );
        assert_eq!(
            core.inspect(&signed).unwrap().action,
            Action::Commit {
                reservation_id: identifier(&operation).unwrap(),
            }
        );
        assert!(
            core.operation(identifier(&operation).unwrap())
                .unwrap()
                .is_none()
        );
        assert!(
            core.operation(identifier(&commit_id).unwrap())
                .unwrap()
                .is_none()
        );
    }
}
