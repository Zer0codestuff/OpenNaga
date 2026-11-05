//! Optimistic concurrency plus an advisory lock prevents two OpenNaga
//! processes from replacing each other's profiles. No mouse backup is
//! handled here.

use super::profile::ProfilesFile;
use super::validation::{self, ProfileDataError, Result};
use std::ffi::CString;
use std::fs;
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

pub struct ProfileFileStore {
    pub path: PathBuf,
    expected: Option<Vec<u8>>,
    loaded: bool,
    read_failed: bool,
}

fn io_error(error: std::io::Error) -> ProfileDataError {
    ProfileDataError(error.to_string())
}

impl ProfileFileStore {
    pub fn new(path: PathBuf) -> Self {
        ProfileFileStore { path, expected: None, loaded: false, read_failed: false }
    }

    pub fn backup_path(&self) -> PathBuf {
        self.path.with_extension("backup.json")
    }

    pub fn folder(&self) -> PathBuf {
        self.path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."))
    }

    pub fn read_document(path: &Path) -> Result<ProfilesFile> {
        decode(&read_bytes(path)?)
    }

    pub fn load(&mut self) -> Result<Option<ProfilesFile>> {
        let result = (|| {
            if !item_exists(&self.path)? {
                return Ok((None, None));
            }
            let data = read_bytes(&self.path)?;
            let file = decode(&data)?;
            Ok((Some(file), Some(data)))
        })();
        match result {
            Ok((file, data)) => {
                self.expected = data;
                self.loaded = true;
                self.read_failed = false;
                Ok(file)
            }
            Err(error) => {
                self.read_failed = true;
                Err(error)
            }
        }
    }

    pub fn save(&mut self, file: &ProfilesFile) -> Result<()> {
        let data = encode(file)?;
        let path = self.path.clone();
        let backup = self.backup_path();
        with_lock(&path, || {
            if self.read_failed {
                return Err(ProfileDataError(
                    "Profiles are protected after a read error. Reload the file or restore its backup before editing.".into(),
                ));
            }
            let exists = item_exists(&path)?;
            let current = if exists { Some(read_bytes(&path)?) } else { None };
            if !(self.loaded || !exists) || current != self.expected {
                return Err(ProfileDataError(
                    "The profiles file changed outside this window. Reload Profiles before saving; your file has not been replaced.".into(),
                ));
            }
            if current.as_deref() == Some(data.as_slice()) {
                self.loaded = true;
                return Ok(());
            }
            // Preserve only a valid previous file. A failed candidate never
            // replaces the primary file and never turns corrupt data into a backup.
            if let Some(current) = &current {
                decode(current)?;
                write_private(current, &backup)?;
            }
            write_private(&data, &path)?;
            self.expected = Some(data.clone());
            self.loaded = true;
            Ok(())
        })
    }

    /// An explicit restore preserves the current file under a unique name,
    /// including corrupt bytes, and leaves the recovery backup intact.
    pub fn restore_backup(&mut self) -> Result<ProfilesFile> {
        let path = self.path.clone();
        let backup = self.backup_path();
        with_lock(&path, || {
            let data = read_bytes(&backup)?;
            let file = decode(&data)?;
            if item_exists(&path)? {
                let meta = fs::symlink_metadata(&path).map_err(io_error)?;
                if !meta.file_type().is_file() {
                    return Err(ProfileDataError("Move the linked or non-file profiles path before restoring a backup.".into()));
                }
                let preserved = path.with_file_name(format!("profiles.before-restore.{}.json", unique_id()));
                fs::copy(&path, &preserved).map_err(io_error)?;
                fs::set_permissions(&preserved, fs::Permissions::from_mode(0o600)).map_err(io_error)?;
            }
            write_private(&data, &path)?;
            self.expected = Some(data);
            self.loaded = true;
            self.read_failed = false;
            Ok(file)
        })
    }

    pub fn has_backup(&self) -> bool {
        self.backup_path().exists()
    }
}

/// Sorted keys and a final version number, like the macOS encoder.
pub fn encode(file: &ProfilesFile) -> Result<Vec<u8>> {
    validation::validate_file(file)?;
    let mut versioned = file.clone();
    versioned.format_version = Some(1);
    // serde_json::Value keeps object keys sorted without `preserve_order`.
    let value = serde_json::to_value(&versioned).map_err(|e| ProfileDataError(e.to_string()))?;
    let mut data = serde_json::to_vec_pretty(&value).map_err(|e| ProfileDataError(e.to_string()))?;
    data.push(b'\n');
    if data.len() > validation::MAXIMUM_FILE_BYTES {
        return Err(ProfileDataError("Profiles exceed the 2 MiB file limit. Export and remove unused profiles first.".into()));
    }
    Ok(data)
}

pub fn decode(data: &[u8]) -> Result<ProfilesFile> {
    let file: ProfilesFile =
        serde_json::from_slice(data).map_err(|e| ProfileDataError(format!("The profiles file is not valid: {e}.")))?;
    validation::validate_file(&file)?;
    Ok(file)
}

fn read_bytes(path: &Path) -> Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path).map_err(io_error)?;
    if !meta.file_type().is_file() {
        return Err(ProfileDataError("Profiles must be stored in a regular file, not a link or folder.".into()));
    }
    if meta.len() as usize > validation::MAXIMUM_FILE_BYTES {
        return Err(ProfileDataError("The profiles file exceeds the 2 MiB limit.".into()));
    }
    let file = fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(path).map_err(io_error)?;
    let mut data = Vec::new();
    file.take(validation::MAXIMUM_FILE_BYTES as u64 + 1).read_to_end(&mut data).map_err(io_error)?;
    if data.len() > validation::MAXIMUM_FILE_BYTES {
        return Err(ProfileDataError("The profiles file exceeds the 2 MiB limit.".into()));
    }
    Ok(data)
}

fn item_exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_error(error)),
    }
}

fn with_lock<T>(path: &Path, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    let folder = path.parent().unwrap_or(Path::new("."));
    create_private_dir(folder)?;
    let mut lock_name = path.as_os_str().to_owned();
    lock_name.push(".lock");
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(PathBuf::from(lock_name))
        .map_err(|_| ProfileDataError("Cannot reserve the profiles file for saving.".into()))?;
    use std::os::fd::AsRawFd;
    let fd = lock.as_raw_fd();
    if unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(ProfileDataError("Another OpenNaga process is saving profiles. Try again.".into()));
    }
    let result = operation();
    unsafe { libc::flock(fd, libc::LOCK_UN) };
    result
}

pub fn create_private_dir(folder: &Path) -> Result<()> {
    if folder.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(folder).map_err(io_error)?;
    fs::set_permissions(folder, fs::Permissions::from_mode(0o700)).map_err(io_error)
}

/// A same-directory rename publishes a fully written, private file. Both the
/// temporary file and the final file have owner-only permissions.
pub fn write_private(data: &[u8], destination: &Path) -> Result<()> {
    let folder = destination.parent().unwrap_or(Path::new("."));
    create_private_dir(folder)?;
    let template = folder.join(".opennaga.XXXXXX");
    let mut template = CString::new(template.as_os_str().as_bytes()).map_err(|e| ProfileDataError(e.to_string()))?.into_bytes_with_nul();
    let fd = unsafe { libc::mkstemp(template.as_mut_ptr() as *mut libc::c_char) };
    if fd < 0 {
        return Err(io_error(std::io::Error::last_os_error()));
    }
    template.pop();
    let temporary = PathBuf::from(std::ffi::OsStr::from_bytes(&template));
    let result = (|| {
        let mut offset = 0;
        while offset < data.len() {
            let count = unsafe { libc::write(fd, data[offset..].as_ptr() as *const libc::c_void, data.len() - offset) };
            if count < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(io_error(error));
            }
            if count == 0 {
                return Err(ProfileDataError("Could not write the file.".into()));
            }
            offset += count as usize;
        }
        if unsafe { libc::fsync(fd) } != 0 {
            return Err(io_error(std::io::Error::last_os_error()));
        }
        fs::rename(&temporary, destination).map_err(io_error)
    })();
    unsafe { libc::close(fd) };
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Atomic, private write for small JSON state files (journals, settings).
pub fn write_json<T: serde::Serialize>(value: &T, destination: &Path) -> Result<()> {
    let data = serde_json::to_vec_pretty(value).map_err(|e| ProfileDataError(e.to_string()))?;
    write_private(&data, destination)
}

fn unique_id() -> String {
    let mut bytes = [0u8; 16];
    if let Ok(mut random) = fs::File::open("/dev/urandom") {
        let _ = random.read_exact(&mut bytes);
    }
    let hex: String = bytes.iter().map(|b| format!("{b:02X}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32])
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::model::profile::default_profiles;
    use std::os::unix::fs::PermissionsExt;

    pub fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("opennaga-test-{}-{}-{}", name, std::process::id(), unique_id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn save_creates_private_file_and_backup() {
        let dir = temp_dir("store");
        let path = dir.join("profiles.json");
        let mut store = ProfileFileStore::new(path.clone());
        assert_eq!(store.load().unwrap(), None);
        let mut file = default_profiles();
        store.save(&file).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert!(!store.has_backup(), "first save has nothing to back up");
        file.profiles.remove("Navigation");
        store.save(&file).unwrap();
        assert!(store.has_backup());
        let backup = ProfileFileStore::read_document(&store.backup_path()).unwrap();
        assert!(backup.profiles.contains_key("Navigation"));
        let text = String::from_utf8(fs::read(&path).unwrap()).unwrap();
        assert!(text.contains("\"formatVersion\": 1"));
        assert!(text.find("\"profiles\"").unwrap() > text.find("\"formatVersion\"").unwrap(), "keys are sorted");
    }

    #[test]
    fn external_change_blocks_saving() {
        let dir = temp_dir("stale");
        let path = dir.join("profiles.json");
        let mut store = ProfileFileStore::new(path.clone());
        store.load().unwrap();
        store.save(&default_profiles()).unwrap();
        let mut other = ProfileFileStore::new(path.clone());
        other.load().unwrap();
        let mut changed = default_profiles();
        changed.profiles.remove("Default");
        other.save(&changed).unwrap();
        let error = store.save(&default_profiles()).unwrap_err();
        assert!(error.0.contains("changed outside"));
        assert_eq!(ProfileFileStore::read_document(&path).unwrap(), {
            let mut v = changed.clone();
            v.format_version = Some(1);
            v
        });
    }

    #[test]
    fn unreadable_file_is_protected_until_restore() {
        let dir = temp_dir("corrupt");
        let path = dir.join("profiles.json");
        let mut store = ProfileFileStore::new(path.clone());
        store.load().unwrap();
        store.save(&default_profiles()).unwrap();
        let mut next = default_profiles();
        next.profiles.remove("Navigation");
        store.save(&next).unwrap();
        fs::write(&path, b"{ not json").unwrap();
        let mut reopened = ProfileFileStore::new(path.clone());
        assert!(reopened.load().is_err());
        assert!(reopened.save(&default_profiles()).unwrap_err().0.contains("protected"));
        assert_eq!(fs::read(&path).unwrap(), b"{ not json", "corrupt bytes are never overwritten by a save");
        let restored = reopened.restore_backup().unwrap();
        assert!(restored.profiles.contains_key("Navigation"));
        let preserved: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("profiles.before-restore."))
            .collect();
        assert_eq!(preserved.len(), 1);
        assert_eq!(fs::read(preserved[0].path()).unwrap(), b"{ not json");
        reopened.save(&next).unwrap();
    }

    #[test]
    fn links_and_oversized_files_are_refused() {
        let dir = temp_dir("links");
        let target = dir.join("real.json");
        fs::write(&target, crate::model::profile::DEFAULT_PROFILES_JSON).unwrap();
        let link = dir.join("profiles.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(ProfileFileStore::new(link).load().unwrap_err().0.contains("regular file"));
        let big = dir.join("big.json");
        fs::write(&big, vec![b' '; validation::MAXIMUM_FILE_BYTES + 1]).unwrap();
        assert!(ProfileFileStore::read_document(&big).unwrap_err().0.contains("2 MiB"));
    }

    #[test]
    fn invalid_documents_never_reach_disk() {
        let dir = temp_dir("invalid");
        let mut store = ProfileFileStore::new(dir.join("profiles.json"));
        store.load().unwrap();
        assert!(store.save(&ProfilesFile::default()).is_err());
        assert!(!dir.join("profiles.json").exists());
    }
}
