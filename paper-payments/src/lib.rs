// SPDX-License-Identifier: GPL-3.0-only
//! Owner-local paper payments using the real core TEST-unit transition executor.
//!
//! The application retains only an immutable signed intent, never a second balance
//! ledger. There is no daemon, network, gateway, real asset, portfolio conversion,
//! saved private key or automatic authorization. Receipts are historical local
//! observations, not distributed finality or evidence of real settlement.

#![forbid(unsafe_code)]

mod disk;

use std::{fs::File, path::Path};

use ed25519_dalek::SigningKey;
use prost::Message;
use sha2::{Digest as _, Sha256};
use volparossa_transaction::{
    Action, Command, LedgerId, Receipt, ReservationState, SignedCommand, Store,
};

const MAX_SIGNED_BYTES: usize = 4096;
const MAX_INTENT_BYTES: usize = 8192;

/// Failures never imply that an already issued core command was rolled back.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Unsafe local files or an unsupported/broken intent envelope.
    #[error("invalid_paper_payment_intent")]
    Invalid,
    /// The original signed command, core domain and retained descriptor disagree.
    #[error("paper_payment_binding_mismatch")]
    Binding,
    /// Core refusal or uncertain local execution; reconcile instead of inventing success.
    #[error("paper_payment_core_error")]
    Core(#[from] volparossa_transaction::Error),
    /// A local filesystem operation failed. No automatic retry or new id is created.
    #[error("paper_payment_io_error")]
    Io(#[from] std::io::Error),
}

/// Explicit owner-selected validity and nonce for one separately signed command.
#[derive(Clone, Copy, Debug)]
pub struct Authorization {
    /// Start, in the core caller-supplied seconds timebase.
    pub valid_from: u64,
    /// Exclusive end in that same timebase.
    pub expires: u64,
    /// New explicit authorization identity, never silently replaced on retry.
    pub nonce: [u8; 32],
}

/// A read-only observation of the one authoritative local core store.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Observation {
    /// No matching record at this observation time. Not a failed-payment claim.
    NotRecorded,
    /// Exact historical receipt; not the current reservation or balance state.
    Recorded(Receipt),
}

#[derive(Clone, PartialEq, Message)]
struct RetainedIntent {
    #[prost(uint32, tag = "1")]
    version: u32,
    #[prost(bytes = "vec", tag = "2")]
    ledger: Vec<u8>,
    #[prost(bytes = "vec", tag = "3")]
    operation: Vec<u8>,
    #[prost(bytes = "vec", tag = "4")]
    signed_sha256: Vec<u8>,
    #[prost(bytes = "vec", tag = "5")]
    signed: Vec<u8>,
}

/// Private, immutable, durably retained intent. This type never persists a key.
pub struct Intent {
    retained: RetainedIntent,
    command: Command,
    _directory: File,
    _file: File,
}

impl Intent {
    /// Sign a new owner-approved command and fsync it before any core mutation.
    /// The supplied core store must independently enroll the financial test key.
    /// The directory must not already exist; existing content is never adopted.
    ///
    /// # Errors
    /// Rejects invalid authorization, unsafe files or a different core domain.
    pub fn prepare(
        path: &Path,
        core: &Store,
        signer: &SigningKey,
        command: Command,
        authorization: Authorization,
    ) -> Result<Self, Error> {
        let ledger = core.ledger_id();
        let command_bytes = SignedCommand::sign(
            ledger,
            signer,
            command,
            authorization.valid_from,
            authorization.expires,
            authorization.nonce,
        )?
        .encode();
        let retained = RetainedIntent {
            version: 1,
            ledger: ledger.to_vec(),
            operation: command.operation_id.to_vec(),
            signed_sha256: Sha256::digest(&command_bytes).to_vec(),
            signed: command_bytes,
        };
        if binding(&retained, core)? != command {
            return Err(Error::Binding);
        }
        let bytes = retained.encode_to_vec();
        let (directory, file) = disk::create(path, &bytes)?;
        Ok(Self {
            retained,
            command,
            _directory: directory,
            _file: file,
        })
    }

    /// Reopen bounded original bytes and verify them with the core before use.
    /// Expiry alone does not invalidate reading an already accepted old intent.
    ///
    /// # Errors
    /// Rejects symlinks, wrong ownership/permissions, noncanonical data or mismatch.
    pub fn open(path: &Path, core: &Store) -> Result<Self, Error> {
        let (directory, file, bytes) = disk::read(path)?;
        let retained = RetainedIntent::decode(bytes.as_slice()).map_err(|_| Error::Invalid)?;
        if retained.encode_to_vec() != bytes {
            return Err(Error::Invalid);
        }
        let command = binding(&retained, core)?;
        Ok(Self {
            retained,
            command,
            _directory: directory,
            _file: file,
        })
    }

    /// Return terms verified from the signed core command, not an app description.
    #[must_use]
    pub fn command(&self) -> Command {
        self.command
    }

    /// Return the exact core domain verified at preparation/opening.
    #[must_use]
    pub fn ledger_id(&self) -> LedgerId {
        // This fixed-length field was checked before constructing Self.
        let mut ledger = [0; 32];
        ledger.copy_from_slice(&self.retained.ledger);
        ledger
    }

    /// Read the authoritative historical result without resubmitting or signing.
    /// Absence remains unknown; this function never fabricates a failure receipt.
    ///
    /// # Errors
    /// Rejects descriptor/core mismatch or a malformed/mismatched stored receipt.
    pub fn reconcile(&self, core: &Store) -> Result<Observation, Error> {
        let command = binding(&self.retained, core)?;
        match core.operation(command.operation_id)? {
            Some(receipt) => {
                self.check_receipt(&receipt)?;
                Ok(Observation::Recorded(receipt))
            }
            None => Ok(Observation::NotRecorded),
        }
    }

    /// Submit the exact retained original bytes, for the first call or a retry.
    /// No new id, nonce, signature, deadline, commit or cancel is generated here.
    /// A core error leaves the outcome to a subsequent explicit reconciliation.
    ///
    /// # Errors
    /// Preserves all core errors including expiry, conflicts, busy and I/O failure.
    pub fn retry(&self, core: &mut Store, now: u64) -> Result<Receipt, Error> {
        binding(&self.retained, core)?;
        let receipt = core.apply(&self.retained.signed, now)?;
        self.check_receipt(&receipt)?;
        Ok(receipt)
    }

    fn check_receipt(&self, receipt: &Receipt) -> Result<(), Error> {
        let (reservation, state) = match self.command.action {
            Action::Reserve { .. } => (self.command.operation_id, ReservationState::Reserved),
            Action::Commit { reservation_id } => (reservation_id, ReservationState::Committed),
            Action::Cancel { reservation_id } => (reservation_id, ReservationState::Cancelled),
        };
        if receipt.ledger_id.as_slice() != self.retained.ledger
            || receipt.operation_id.as_slice() != self.retained.operation
            || receipt.command_sha256.as_slice() != self.retained.signed_sha256
            || receipt.sequence == 0
            || receipt.reservation_id != reservation
            || receipt.state != state
        {
            return Err(Error::Binding);
        }
        Ok(())
    }
}

fn binding(retained: &RetainedIntent, core: &Store) -> Result<Command, Error> {
    if retained.version != 1
        || retained.ledger != core.ledger_id()
        || retained.operation.len() != 32
        || retained.signed.is_empty()
        || retained.signed.len() > MAX_SIGNED_BYTES
        || retained.signed_sha256 != Sha256::digest(&retained.signed).as_slice()
    {
        return Err(Error::Binding);
    }
    // Only core interprets and authenticates its signed transaction protocol.
    // This happens before any mutation, including after reopening untrusted files.
    let command = core.inspect(&retained.signed)?;
    if retained.operation != command.operation_id {
        return Err(Error::Binding);
    }
    Ok(command)
}

#[cfg(test)]
mod tests;
