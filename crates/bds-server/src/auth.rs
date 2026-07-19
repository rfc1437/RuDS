use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use russh::keys::key::safe_rng;
use russh::keys::{Algorithm, PrivateKey, PublicKey, load_secret_key, ssh_key};

pub const HOST_KEY_FILE: &str = "ssh_host_rsa_key";
pub const AUTHORIZED_KEYS_FILE: &str = "authorized_keys";
pub const CLIENT_KEY_FILE: &str = "id_ed25519";
pub const KNOWN_HOSTS_FILE: &str = "known_hosts";

#[derive(Debug, Clone)]
pub struct KeyMaterial {
    pub directory: PathBuf,
    pub host_key_path: PathBuf,
    pub authorized_keys_path: PathBuf,
}

impl KeyMaterial {
    pub fn ensure(data_dir: &Path) -> Result<Self> {
        let directory = data_dir.join("ssh");
        ensure_private_directory(&directory)?;
        let host_key_path = directory.join(HOST_KEY_FILE);
        let authorized_keys_path = directory.join(AUTHORIZED_KEYS_FILE);
        ensure_private_file(&authorized_keys_path, b"")?;
        if !host_key_path.exists() {
            let key = PrivateKey::random(&mut safe_rng(), Algorithm::Rsa { hash: None })
                .context("could not generate the SSH host key")?;
            let encoded = key
                .to_openssh(ssh_key::LineEnding::LF)
                .context("could not encode the SSH host key")?;
            ensure_private_file(&host_key_path, encoded.as_bytes())?;
        }
        validate_private_file(&host_key_path)?;
        validate_private_file(&authorized_keys_path)?;
        load_secret_key(&host_key_path, None)
            .with_context(|| format!("could not read SSH host key {}", host_key_path.display()))?;
        Ok(Self {
            directory,
            host_key_path,
            authorized_keys_path,
        })
    }

    pub fn host_key(&self) -> Result<PrivateKey> {
        validate_private_file(&self.host_key_path)?;
        load_secret_key(&self.host_key_path, None).with_context(|| {
            format!(
                "could not read SSH host key {}",
                self.host_key_path.display()
            )
        })
    }

    /// Re-reads the file for every authentication attempt, so removing a key
    /// revokes it without restarting the server.
    pub fn authorizes(&self, candidate: &PublicKey) -> Result<bool> {
        validate_private_file(&self.authorized_keys_path)?;
        let contents = fs::read_to_string(&self.authorized_keys_path).with_context(|| {
            format!(
                "could not read authorized keys {}",
                self.authorized_keys_path.display()
            )
        })?;
        for (index, line) in contents.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let fields = line.split_whitespace().collect::<Vec<_>>();
            let key_start = fields
                .iter()
                .position(|field| {
                    field.starts_with("ssh-")
                        || field.starts_with("ecdsa-")
                        || field.starts_with("sk-")
                })
                .filter(|index| fields.get(index + 1).is_some());
            let encoded = key_start
                .map(|index| format!("{} {}", fields[index], fields[index + 1]))
                .unwrap_or_else(|| line.to_owned());
            let key = PublicKey::from_openssh(&encoded).with_context(|| {
                format!(
                    "invalid authorized key at {}:{}",
                    self.authorized_keys_path.display(),
                    index + 1
                )
            })?;
            if key.key_data() == candidate.key_data() {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[derive(Debug, Clone)]
pub struct ClientKeyMaterial {
    pub directory: PathBuf,
    pub private_key_path: PathBuf,
    pub public_key_path: PathBuf,
    pub known_hosts_path: PathBuf,
}

impl ClientKeyMaterial {
    pub fn ensure(data_dir: &Path) -> Result<Self> {
        let directory = data_dir.join("ssh");
        ensure_private_directory(&directory)?;
        let private_key_path = directory.join(CLIENT_KEY_FILE);
        let public_key_path = directory.join(format!("{CLIENT_KEY_FILE}.pub"));
        let known_hosts_path = directory.join(KNOWN_HOSTS_FILE);
        ensure_private_file(&known_hosts_path, b"")?;
        if !private_key_path.exists() {
            let mut key = PrivateKey::random(&mut safe_rng(), Algorithm::Ed25519)
                .context("could not generate the SSH client identity")?;
            key.set_comment("ruds-desktop");
            let private = key
                .to_openssh(ssh_key::LineEnding::LF)
                .context("could not encode the SSH client identity")?;
            ensure_private_file(&private_key_path, private.as_bytes())?;
            let public = format!("{}\n", key.public_key().to_openssh()?);
            ensure_public_file(&public_key_path, public.as_bytes())?;
        }
        validate_private_file(&private_key_path)?;
        validate_private_file(&known_hosts_path)?;
        let key = load_secret_key(&private_key_path, None).with_context(|| {
            format!(
                "could not read SSH client identity {}",
                private_key_path.display()
            )
        })?;
        let public = format!("{}\n", key.public_key().to_openssh()?);
        ensure_public_file(&public_key_path, public.as_bytes())?;
        Ok(Self {
            directory,
            private_key_path,
            public_key_path,
            known_hosts_path,
        })
    }
}

fn ensure_private_directory(path: &Path) -> Result<()> {
    if !path.exists() {
        fs::create_dir_all(path).with_context(|| {
            format!("could not create private SSH directory {}", path.display())
        })?;
        set_mode(path, 0o700)?;
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!("SSH key directory is not a directory: {}", path.display());
    }
    validate_mode(path, 0o700, "SSH key directory")
}

fn ensure_private_file(path: &Path, contents: &[u8]) -> Result<()> {
    if path.exists() {
        return validate_private_file(path);
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .with_context(|| format!("could not create private SSH file {}", path.display()))?;
    file.write_all(contents)?;
    file.sync_all()?;
    set_mode(path, 0o600)?;
    Ok(())
}

fn ensure_public_file(path: &Path, contents: &[u8]) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(contents)?;
    file.sync_all()?;
    set_mode(path, 0o644)
}

fn validate_private_file(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!(
            "SSH key file is missing or not a regular file: {}",
            path.display()
        );
    }
    validate_mode(path, 0o600, "SSH key file")
}

#[cfg(unix)]
fn validate_mode(path: &Path, maximum: u32, label: &str) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    let mode = fs::metadata(path)?.permissions().mode() & 0o777;
    if mode & !maximum != 0 {
        bail!(
            "unsafe permissions {mode:o} on {label} {}; expected {maximum:o}",
            path.display()
        );
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_mode(_path: &Path, _maximum: u32, _label: &str) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_and_reuses_restrictive_server_key_material() {
        let root = tempfile::tempdir().unwrap();
        let first = KeyMaterial::ensure(root.path()).unwrap();
        let bytes = fs::read(&first.host_key_path).unwrap();
        assert_eq!(
            first.host_key().unwrap().algorithm(),
            Algorithm::Rsa { hash: None }
        );
        assert_eq!(fs::read_to_string(&first.authorized_keys_path).unwrap(), "");
        let second = KeyMaterial::ensure(root.path()).unwrap();
        assert_eq!(fs::read(second.host_key_path).unwrap(), bytes);
        assert_private(&first.directory, 0o700);
        assert_private(&first.host_key_path, 0o600);
        assert_private(&first.authorized_keys_path, 0o600);
    }

    #[test]
    fn authorized_keys_accept_reject_and_revoke_immediately() {
        let root = tempfile::tempdir().unwrap();
        let material = KeyMaterial::ensure(root.path()).unwrap();
        let allowed = PrivateKey::random(&mut safe_rng(), Algorithm::Ed25519).unwrap();
        let unknown = PrivateKey::random(&mut safe_rng(), Algorithm::Ed25519).unwrap();
        fs::write(
            &material.authorized_keys_path,
            format!(
                "# desktop\nrestrict {} user@desktop\n",
                allowed.public_key().to_openssh().unwrap()
            ),
        )
        .unwrap();
        assert!(material.authorizes(allowed.public_key()).unwrap());
        assert!(!material.authorizes(unknown.public_key()).unwrap());
        fs::write(&material.authorized_keys_path, "").unwrap();
        assert!(!material.authorizes(allowed.public_key()).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn unsafe_key_files_are_rejected_with_the_path_and_mode() {
        use std::os::unix::fs::PermissionsExt as _;
        let root = tempfile::tempdir().unwrap();
        let material = KeyMaterial::ensure(root.path()).unwrap();
        fs::set_permissions(
            &material.authorized_keys_path,
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        let error = material
            .authorizes(
                PrivateKey::random(&mut safe_rng(), Algorithm::Ed25519)
                    .unwrap()
                    .public_key(),
            )
            .unwrap_err()
            .to_string();
        assert!(error.contains("unsafe permissions 644"));
        assert!(error.contains(AUTHORIZED_KEYS_FILE));
    }

    #[test]
    fn creates_a_reusable_desktop_identity_and_known_hosts() {
        let root = tempfile::tempdir().unwrap();
        let first = ClientKeyMaterial::ensure(root.path()).unwrap();
        let bytes = fs::read(&first.private_key_path).unwrap();
        fs::remove_file(&first.public_key_path).unwrap();
        let second = ClientKeyMaterial::ensure(root.path()).unwrap();
        assert_eq!(fs::read(second.private_key_path).unwrap(), bytes);
        assert!(first.public_key_path.is_file());
        assert_private(&first.private_key_path, 0o600);
        assert_private(&first.known_hosts_path, 0o600);
    }

    fn assert_private(path: &Path, expected: u32) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                expected
            );
        }
    }
}
