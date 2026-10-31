//! Worker-only persistence of an explicitly selected, regular document.
//! Does not discover files, interpret audio, store URIs or alter the source.
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub const MAX_DOCUMENT_BYTES: u64 = 256 * 1024 * 1024;
static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Cancelled,
    Deadline,
    TooLarge,
    Input,
    Conflict,
}

struct Staging(PathBuf);
impl Drop for Staging {
    fn drop(&mut self) {
        // Only names exclusively created by this invocation. No recursive GC.
        let _ = fs::remove_file(self.0.join("source.wav"));
        let _ = fs::remove_dir(&self.0);
    }
}

fn private_directory(path: &Path) -> Result<(), Error> {
    #[cfg(unix)]
    let result = {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().mode(0o700).create(path)
    };
    #[cfg(not(unix))]
    let result = fs::create_dir(path);
    result.map_err(|_| Error::Input)
}

fn hash_file(
    file: &mut File,
    limit: u64,
    check: &mut impl FnMut() -> Result<(), Error>,
    mut copy: Option<&mut File>,
) -> Result<(u64, String), Error> {
    let mut digest = Sha256::new();
    let mut bytes = [0_u8; 8192];
    let mut size = 0_u64;
    loop {
        check()?;
        let count = file.read(&mut bytes).map_err(|_| Error::Input)?;
        if count == 0 {
            break;
        }
        size = size.checked_add(count as u64).ok_or(Error::TooLarge)?;
        if size > limit {
            return Err(Error::TooLarge);
        }
        digest.update(&bytes[..count]);
        if let Some(output) = copy.as_mut() {
            output
                .write_all(&bytes[..count])
                .map_err(|_| Error::Input)?;
        }
    }
    Ok((size, format!("{:x}", digest.finalize())))
}

/// Call only from a worker with a user-selected descriptor. Pipes and oversized
/// files are rejected before copying. The caller validates the resulting audio
/// before saving neutral metadata. Identical bytes reuse the private path.
pub fn store(
    source: File,
    root: &Path,
    check: impl FnMut() -> Result<(), Error>,
) -> Result<PathBuf, Error> {
    store_with_limit(source, root, MAX_DOCUMENT_BYTES, check)
}

fn store_with_limit(
    mut source: File,
    root: &Path,
    limit: u64,
    mut check: impl FnMut() -> Result<(), Error>,
) -> Result<PathBuf, Error> {
    check()?;
    let info = source.metadata().map_err(|_| Error::Input)?;
    if !info.is_file() {
        return Err(Error::Input);
    }
    if info.len() > limit {
        return Err(Error::TooLarge);
    }
    source.seek(SeekFrom::Start(0)).map_err(|_| Error::Input)?;
    if !root.try_exists().map_err(|_| Error::Input)? {
        private_directory(root)?;
    }
    if !fs::symlink_metadata(root)
        .map_err(|_| Error::Input)?
        .is_dir()
    {
        return Err(Error::Input);
    }
    let staging = (0..4)
        .find_map(|_| {
            let path = root.join(format!(
                ".pending-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            private_directory(&path).ok().map(|()| Staging(path))
        })
        .ok_or(Error::Input)?;
    let temporary = staging.0.join("source.wav");
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut copy = options.open(&temporary).map_err(|_| Error::Input)?;
    let (size, hash) = hash_file(&mut source, limit, &mut check, Some(&mut copy))?;
    copy.sync_all().map_err(|_| Error::Input)?;
    drop(copy);
    check()?;
    let destination = root.join(hash);
    let path = destination.join("source.wav");
    match private_directory(&destination) {
        Ok(()) => {
            // A new, exclusively owned directory avoids hard links (denied by
            // some Android sandboxes) and never replaces an existing file.
            if fs::rename(&temporary, &path).is_err() {
                let _ = fs::remove_dir(&destination);
                return Err(Error::Input);
            }
            File::open(&destination)
                .and_then(|f| f.sync_all())
                .map_err(|_| Error::Input)?;
            File::open(root)
                .and_then(|f| f.sync_all())
                .map_err(|_| Error::Input)?;
        }
        Err(_) => {
            if !fs::symlink_metadata(&destination)
                .map_err(|_| Error::Conflict)?
                .is_dir()
                || !fs::symlink_metadata(&path)
                    .map_err(|_| Error::Conflict)?
                    .is_file()
            {
                return Err(Error::Conflict);
            }
            let mut previous = File::open(&path).map_err(|_| Error::Conflict)?;
            let (old_size, old_hash) = hash_file(&mut previous, limit, &mut check, None)?;
            if size != old_size || destination.file_name().unwrap() != old_hash.as_str() {
                return Err(Error::Conflict);
            }
        }
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn profile() -> Staging {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!(
                "document-copy-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir(&root).unwrap();
        Staging(root)
    }
    #[test]
    fn bounded_copy_deduplicates_bytes_and_preserves_source_and_unknown_files() {
        let profile = profile();
        let source = profile.0.join("source.wav");
        let original = b"own fixture, not audio decoding in the persistence adapter";
        fs::write(&source, original).unwrap();
        let root = profile.0.join("documents");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("unrelated"), b"preserve").unwrap();
        let first = store(File::open(&source).unwrap(), &root, || Ok(())).unwrap();
        let again = store(File::open(&source).unwrap(), &root, || Ok(())).unwrap();
        assert_eq!(first, again);
        assert_eq!(fs::read(&first).unwrap(), original);
        assert_eq!(fs::read(&source).unwrap(), original);
        assert_eq!(fs::read(root.join("unrelated")).unwrap(), b"preserve");
        assert!(!fs::read_dir(&root).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".pending-")
        }));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&first).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        // Corrupt only our own destination to demonstrate collision protection.
        fs::write(&first, b"own corrupted copy").unwrap();
        assert_eq!(
            store(File::open(&source).unwrap(), &root, || Ok(())),
            Err(Error::Conflict)
        );
        assert_eq!(fs::read(&first).unwrap(), b"own corrupted copy");
        fs::remove_dir_all(&root).unwrap();
    }
    #[test]
    fn size_and_cancellation_fail_without_publishing_or_touching_source() {
        let profile = profile();
        let source = profile.0.join("source.wav");
        fs::write(&source, [42; 16384]).unwrap();
        let root = profile.0.join("documents");
        assert_eq!(
            store_with_limit(File::open(&source).unwrap(), &root, 8, || Ok(())),
            Err(Error::TooLarge)
        );
        assert!(!root.exists());
        let mut checks = 0;
        assert_eq!(
            store(File::open(&source).unwrap(), &root, || {
                checks += 1;
                if checks >= 3 {
                    Err(Error::Cancelled)
                } else {
                    Ok(())
                }
            }),
            Err(Error::Cancelled)
        );
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
        assert_eq!(fs::read(&source).unwrap(), vec![42; 16384]);
        fs::remove_dir(&root).unwrap();
    }
}
