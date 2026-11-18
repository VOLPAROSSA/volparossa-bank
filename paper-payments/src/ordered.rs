// SPDX-License-Identifier: GPL-3.0-only
//! Durable ordered TEST intents; ordering, execution and finality remain outside Bank.

use std::{
    fs::{File, Metadata},
    os::unix::fs::MetadataExt as _,
    path::{Path, PathBuf},
};

use ed25519_dalek::SigningKey;
use prost::Message;
use sha2::{Digest as _, Sha256};
use volparossa_transaction::{Command, LedgerId, OrderedStore, SignedCommand};

use crate::{
    Authorization, Error, MAX_SIGNED_BYTES, Observation, RetainedIntent, check_receipt, disk,
};

const ORDERED_INTENT_VERSION: u32 = 2;

#[derive(Clone, Copy, Eq, PartialEq)]
struct Fingerprint {
    device: u64,
    inode: u64,
    mode: u32,
    owner: u32,
    group: u32,
    links: u64,
    bytes: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}
impl From<Metadata> for Fingerprint {
    fn from(value: Metadata) -> Self {
        Self {
            device: value.dev(),
            inode: value.ino(),
            mode: value.mode(),
            owner: value.uid(),
            group: value.gid(),
            links: value.nlink(),
            bytes: value.len(),
            modified: (value.mtime(), value.mtime_nsec()),
            changed: (value.ctime(), value.ctime_nsec()),
        }
    }
}

/// Private immutable intent for Core's separately signed ordered TEST domain.
///
/// Version 2 is distinct from the unchanged owner-local `Intent` envelope (v1).
/// Only the original command is retained: no key, balance or receipt cache. This
/// object cannot stage/commit a block, choose validators or certify finality.
/// Private file permissions are not encryption or protection from an administrator.
pub struct OrderedIntent {
    retained: RetainedIntent,
    command: Command,
    path: PathBuf,
    directory: File,
    file: File,
    directory_fingerprint: Fingerprint,
    file_fingerprint: Fingerprint,
}

impl OrderedIntent {
    /// Sign exact owner-approved ordered terms, then fsync the private intent.
    ///
    /// The financial TEST key must independently match an enrolled payer. The
    /// original ordered signature binds the genesis-derived ledger. No submission
    /// bytes are exposed until the existing file/directory/parent syncs succeed.
    ///
    /// # Errors
    /// Rejects invalid authorization, wrong domain/enrollment, existing paths or
    /// unsafe private files. Creation failure does not authorize submission.
    pub fn prepare(
        path: &Path,
        core: &OrderedStore,
        signer: &SigningKey,
        command: Command,
        authorization: Authorization,
    ) -> Result<Self, Error> {
        let command_bytes = SignedCommand::sign_ordered(
            core.ledger_id(),
            signer,
            command,
            authorization.valid_from,
            authorization.expires,
            authorization.nonce,
        )?
        .encode();
        let retained = RetainedIntent {
            version: ORDERED_INTENT_VERSION,
            ledger: core.ledger_id().to_vec(),
            operation: command.operation_id.to_vec(),
            signed_sha256: Sha256::digest(&command_bytes).to_vec(),
            signed: command_bytes,
        };
        if binding(&retained, core)? != command {
            return Err(Error::Binding);
        }
        let (directory, file) = disk::create(path, &retained.encode_to_vec())?;
        Self::captured(path, retained, command, directory, file)
    }

    /// Reopen the canonical private ordered intent without renewing its authority.
    /// Expired original bytes remain inspectable; only Core decides execution.
    ///
    /// # Errors
    /// Rejects local-mode envelopes, wrong domain/enrollment, unsafe files,
    /// noncanonical encoding or altered signed bytes/descriptor bindings.
    pub fn open(path: &Path, core: &OrderedStore) -> Result<Self, Error> {
        let (directory, file, bytes) = disk::read(path)?;
        let retained = RetainedIntent::decode(bytes.as_slice()).map_err(|_| Error::Invalid)?;
        if retained.encode_to_vec() != bytes {
            return Err(Error::Invalid);
        }
        let command = binding(&retained, core)?;
        Self::captured(path, retained, command, directory, file)
    }

    fn captured(
        path: &Path,
        retained: RetainedIntent,
        command: Command,
        directory: File,
        file: File,
    ) -> Result<Self, Error> {
        let directory_fingerprint = directory.metadata()?.into();
        let file_fingerprint = file.metadata()?.into();
        Ok(Self {
            retained,
            command,
            path: path.to_path_buf(),
            directory,
            file,
            directory_fingerprint,
            file_fingerprint,
        })
    }

    /// Original verified financial TEST terms, not execution or finality evidence.
    #[must_use]
    pub fn command(&self) -> Command {
        self.command
    }

    /// Exact genesis-derived ordered ledger binding; not a consensus certificate.
    #[must_use]
    pub fn ledger_id(&self) -> LedgerId {
        let mut ledger = [0; 32];
        ledger.copy_from_slice(&self.retained.ledger);
        ledger
    }

    /// Borrow the exact durably retained signed bytes for explicit submission/retry.
    ///
    /// No new operation id, nonce, signature or validity window is created, even
    /// after expiry. A caller must use Core's reviewed ordering/execution path;
    /// this adapter cannot apply, stage or commit. Re-checking the private file is
    /// an admission snapshot, not continuous tamper protection after returning.
    ///
    /// # Errors
    /// Rejects replaced/mutated files and any domain, signature or payer mismatch.
    pub fn submission_bytes(&self, core: &OrderedStore) -> Result<&[u8], Error> {
        self.verify(core)?;
        Ok(&self.retained.signed)
    }

    /// Read only the committed historical Core receipt; staged outcomes are absent.
    ///
    /// `NotRecorded` is an observation, not a failed-transfer claim. `Recorded`
    /// is local persistence, not verified consensus, current reservation state or
    /// external settlement. No resubmission, signing or mutation occurs here.
    ///
    /// # Errors
    /// Rejects changed private intent, domain/enrollment or receipt mismatch.
    pub fn reconcile(&self, core: &OrderedStore) -> Result<Observation, Error> {
        let command = self.verify(core)?;
        match core.operation(command.operation_id)? {
            Some(receipt) => {
                check_receipt(&self.retained, command, &receipt)?;
                Ok(Observation::Recorded(receipt))
            }
            None => Ok(Observation::NotRecorded),
        }
    }

    fn verify(&self, core: &OrderedStore) -> Result<Command, Error> {
        // Reuse the existing nofollow/owned/single-link/private/bounded reader.
        // Compare held and freshly opened descriptors to the admitted identity;
        // replacement or metadata/content mutation cannot redirect a retry.
        let (directory, file, bytes) = disk::read(&self.path)?;
        if Fingerprint::from(self.directory.metadata()?) != self.directory_fingerprint
            || Fingerprint::from(directory.metadata()?) != self.directory_fingerprint
            || Fingerprint::from(self.file.metadata()?) != self.file_fingerprint
            || Fingerprint::from(file.metadata()?) != self.file_fingerprint
            || bytes != self.retained.encode_to_vec()
        {
            return Err(Error::Binding);
        }
        binding(&self.retained, core)
    }
}

fn binding(retained: &RetainedIntent, core: &OrderedStore) -> Result<Command, Error> {
    if retained.version != ORDERED_INTENT_VERSION
        || retained.ledger != core.ledger_id()
        || retained.operation.len() != 32
        || retained.signed.is_empty()
        || retained.signed.len() > MAX_SIGNED_BYTES
        || retained.signed_sha256 != Sha256::digest(&retained.signed).as_slice()
    {
        return Err(Error::Binding);
    }
    // Only Core parses/authenticates its canonical ordered signature domain.
    let command = core.inspect(&retained.signed)?;
    if retained.operation != command.operation_id {
        return Err(Error::Binding);
    }
    Ok(command)
}

#[cfg(test)]
#[path = "ordered_tests.rs"]
mod tests;
