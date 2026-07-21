use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

/// Compute a hex-encoded SHA-256 hash of the given content.
pub fn content_hash(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    let result = hasher.finalize();
    encode_hex(result)
}

/// Compute a hex-encoded SHA-256 hash of a file by streaming (8 KB chunks).
pub fn file_hash(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(encode_hex(hasher.finalize()))
}

/// Compute the bDS2-compatible lower-hex MD5 change signal for media bytes.
pub fn media_content_hash(content: &[u8]) -> String {
    format!("{:x}", md5::compute(content))
}

/// Compute the bDS2-compatible lower-hex MD5 change signal for a media file.
pub fn media_file_hash(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut context = md5::Context::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        context.consume(&buf[..n]);
    }
    Ok(format!("{:x}", context.finalize()))
}

fn encode_hex(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_hash() {
        // SHA-256 of "hello"
        let hash = content_hash(b"hello");
        assert_eq!(
            hash,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn empty_hash() {
        let hash = content_hash(b"");
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn media_hash_matches_bds2_fixture() {
        // Base.encode16(:crypto.hash(:md5, "hello"), case: :lower) in bDS2.
        assert_eq!(
            media_content_hash(b"hello"),
            "5d41402abc4b2a76b9719d911017c592"
        );
    }
}
