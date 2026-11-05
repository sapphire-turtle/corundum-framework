//! Checking a release file against its minisign signature.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use minisign_verify::{PublicKey, Signature};

use crate::{Error, Result};

/// Check `bytes`, the release file `name`, against `signature` with the public key `pubkey`.
///
/// Both are base64, as cargo-packager writes them: `pubkey` of the `.pub` file, `signature` of
/// the signature file (the content of `<file>.sig`). The signature's trusted comment must name
/// the file (`file:<name>`), so a file signed for another app or another platform with the
/// same key is not taken for this one.
pub fn verify_signature(bytes: &[u8], signature: &str, pubkey: &str, name: &str) -> Result<()> {
    let bad = |what: String| Error::Signature(format!("{name}: {what}"));
    let pubkey = decode(pubkey)
        .map_err(|e| Error::NotConfigured(format!("the public key {name} is checked with: {e}")))?;
    let pubkey = PublicKey::decode(&pubkey)
        .map_err(|e| Error::NotConfigured(format!("the public key {name} is checked with: {e}")))?;
    let signature = decode(signature).map_err(|e| bad(format!("unreadable signature: {e}")))?;
    let signature =
        Signature::decode(&signature).map_err(|e| bad(format!("unreadable signature: {e}")))?;
    // Prehashed signatures only: cargo-packager (minisign 0.7) writes no other kind.
    pubkey
        .verify(bytes, &signature, false)
        .map_err(|e| bad(format!("the signature does not match: {e}")))?;
    let signed_for = signature
        .trusted_comment()
        .split('\t')
        .find_map(|field| field.strip_prefix("file:"));
    if signed_for != Some(name) {
        return Err(bad(format!(
            "the signature is for {}",
            signed_for.unwrap_or("no file named")
        )));
    }
    Ok(())
}

/// The text inside base64 `encoded`, with surrounding whitespace ignored.
fn decode(encoded: &str) -> std::result::Result<String, String> {
    let bytes = STANDARD
        .decode(encoded.trim())
        .map_err(|e| format!("not base64: {e}"))?;
    String::from_utf8(bytes).map_err(|e| format!("not text: {e}"))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A key pair as cargo-packager keeps it: the public key, base64 of its `.pub` file.
    pub(crate) struct Key {
        pk: minisign::PublicKey,
        sk: minisign::SecretKey,
        /// The base64 of the `.pub` file.
        pub(crate) public: String,
    }

    impl Key {
        pub(crate) fn new() -> Key {
            let minisign::KeyPair { pk, sk } =
                minisign::KeyPair::generate_unencrypted_keypair().unwrap();
            let public = STANDARD.encode(pk.to_box().unwrap().to_string());
            Key { pk, sk, public }
        }

        /// `bytes` signed as `cargo packager signer sign` signs the file `name`: the content
        /// of `<name>.sig`.
        pub(crate) fn sign(&self, bytes: &[u8], name: &str) -> String {
            let trusted = format!("timestamp:1762300000\tfile:{name}");
            let signature = minisign::sign(
                Some(&self.pk),
                &self.sk,
                bytes,
                Some(&trusted),
                Some("signature from cargo-packager secret key"),
            )
            .unwrap();
            STANDARD.encode(signature.to_string())
        }
    }

    #[test]
    fn a_file_signed_with_the_key_passes() {
        let key = Key::new();
        let signature = key.sign(b"binary", "app-linux-x86_64");
        verify_signature(b"binary", &signature, &key.public, "app-linux-x86_64").unwrap();
        // The file as it comes, with the trailing newline a `.sig` may have.
        verify_signature(
            b"binary",
            &format!("{signature}\n"),
            &key.public,
            "app-linux-x86_64",
        )
        .unwrap();
    }

    #[test]
    fn a_changed_file_is_refused() {
        let key = Key::new();
        let signature = key.sign(b"binary", "app-linux-x86_64");
        let err =
            verify_signature(b"binarY", &signature, &key.public, "app-linux-x86_64").unwrap_err();
        assert!(matches!(err, Error::Signature(_)), "{err}");
    }

    #[test]
    fn another_key_s_signature_is_refused() {
        let signature = Key::new().sign(b"binary", "app-linux-x86_64");
        let err = verify_signature(
            b"binary",
            &signature,
            &Key::new().public,
            "app-linux-x86_64",
        )
        .unwrap_err();
        assert!(matches!(err, Error::Signature(_)), "{err}");
    }

    #[test]
    fn a_file_signed_under_another_name_is_refused() {
        let key = Key::new();
        let signature = key.sign(b"binary", "other-app-linux-x86_64");
        let err =
            verify_signature(b"binary", &signature, &key.public, "app-linux-x86_64").unwrap_err();
        assert!(matches!(err, Error::Signature(_)), "{err}");
        assert!(err.to_string().contains("other-app-linux-x86_64"), "{err}");
    }

    #[test]
    fn a_garbled_signature_is_refused_and_a_garbled_key_is_the_app_s_fault() {
        let key = Key::new();
        let err = verify_signature(b"binary", "not a signature", &key.public, "app").unwrap_err();
        assert!(matches!(err, Error::Signature(_)), "{err}");
        let signature = key.sign(b"binary", "app");
        let err = verify_signature(b"binary", &signature, "bm90IGEga2V5", "app").unwrap_err();
        assert!(matches!(err, Error::NotConfigured(_)), "{err}");
    }
}
