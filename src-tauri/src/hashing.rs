use sha2::{Digest, Sha256};

/// Preserve the lowercase, zero-padded SHA-256 wire format across digest versions.
pub(crate) fn sha256_hex(input: impl AsRef<[u8]>) -> String {
    Sha256::digest(input.as_ref())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_vectors_preserve_leading_zeroes_and_lowercase() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
