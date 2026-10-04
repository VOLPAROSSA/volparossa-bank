// SPDX-License-Identifier: GPL-3.0-only
//! Owner-private bounded immutable intent files, with no key or receipt cache.

use std::{
    fs::{self, File},
    io::{Read as _, Write as _},
    os::unix::fs::{DirBuilderExt as _, MetadataExt as _},
    path::Path,
};

use rustix::fs::{Mode, OFlags};

use crate::{Error, MAX_INTENT_BYTES};

const NAME: &str = "intent.pb";

fn directory(path: &Path) -> Result<File, Error> {
    if !path.is_absolute() || path.as_os_str().len() > 4096 || path.canonicalize()? != path {
        return Err(Error::Invalid);
    }
    let file = File::from(
        rustix::fs::open(
            path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(std::io::Error::from)?,
    );
    let metadata = file.metadata()?;
    if metadata.uid() != rustix::process::geteuid().as_raw() || metadata.mode() & 0o777 != 0o700 {
        return Err(Error::Invalid);
    }
    Ok(file)
}

fn same_file(file: &File, path: &Path) -> Result<(), Error> {
    let before = file.metadata()?;
    let after = fs::symlink_metadata(path)?;
    if after.file_type().is_symlink()
        || before.dev() != after.dev()
        || before.ino() != after.ino()
        || before.uid() != after.uid()
    {
        return Err(Error::Invalid);
    }
    Ok(())
}

pub(super) fn create(path: &Path, bytes: &[u8]) -> Result<(File, File), Error> {
    if !path.is_absolute() || bytes.is_empty() || bytes.len() > MAX_INTENT_BYTES {
        return Err(Error::Invalid);
    }
    // Reject a symlinked parent before creating anything through it.
    let parent = path.parent().ok_or(Error::Invalid)?;
    if parent.canonicalize()? != parent || path.file_name().is_none() {
        return Err(Error::Invalid);
    }
    fs::DirBuilder::new().mode(0o700).create(path)?;
    let root = directory(path)?;
    let mut file = File::from(
        rustix::fs::openat(
            &root,
            NAME,
            OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )
        .map_err(std::io::Error::from)?,
    );
    let metadata = file.metadata()?;
    if metadata.mode() & 0o777 != 0o600 || metadata.nlink() != 1 {
        return Err(Error::Invalid);
    }
    file.write_all(bytes)?;
    file.sync_all()?;
    same_file(&root, path)?;
    same_file(&file, &path.join(NAME))?;
    root.sync_all()?;
    File::open(parent)?.sync_all()?;
    Ok((root, file))
}

pub(super) fn read(path: &Path) -> Result<(File, File, Vec<u8>), Error> {
    let root = directory(path)?;
    let mut file = File::from(
        rustix::fs::openat(
            &root,
            NAME,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(std::io::Error::from)?,
    );
    let before = file.metadata()?;
    if !before.is_file()
        || before.uid() != rustix::process::geteuid().as_raw()
        || before.nlink() != 1
        || before.mode() & 0o777 != 0o600
        || before.len() == 0
        || before.len() > MAX_INTENT_BYTES as u64
    {
        return Err(Error::Invalid);
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(MAX_INTENT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 != before.len() || file.metadata()?.len() != before.len() {
        return Err(Error::Invalid);
    }
    same_file(&root, path)?;
    same_file(&file, &path.join(NAME))?;
    Ok((root, file, bytes))
}
