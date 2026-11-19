//! Machine-local OpenPGP key that signs every `repomd.xml` quaid serves.
//!
//! dnf checks the signature (`repo_gpgcheck=1`), so a process that manages to
//! answer on quaid's port still cannot feed dnf forged metadata.

use anyhow::{Context, Result};
use pgp::composed::{
    ArmorOptions, Deserializable, DetachedSignature, KeyType, SecretKeyParamsBuilder, SignedPublicKey,
    SignedSecretKey,
};
use pgp::crypto::hash::HashAlgorithm;
use pgp::types::{KeyDetails, Password};
use std::path::Path;

pub struct Signer {
    key: SignedSecretKey,
}

pub struct GeneratedKey {
    pub signer: Signer,
    pub secret_armored: String,
    pub public_armored: String,
}

impl Signer {
    /// New Ed25519 (v4, widely supported) signing key with the given user id.
    pub fn generate(user_id: &str) -> Result<GeneratedKey> {
        let mut params = SecretKeyParamsBuilder::default();
        params
            .key_type(KeyType::Ed25519Legacy)
            .can_certify(true)
            .can_sign(true)
            .primary_user_id(user_id.to_string());
        let params = params.build().context("building key parameters")?;
        let key = params.generate(rand::thread_rng()).context("generating signing key")?;
        let secret_armored = key.to_armored_string(ArmorOptions::default()).context("armoring secret key")?;
        let public = SignedPublicKey::from(key.clone());
        let public_armored = public.to_armored_string(ArmorOptions::default()).context("armoring public key")?;
        Ok(GeneratedKey { signer: Signer { key }, secret_armored, public_armored })
    }

    pub fn from_armored(text: &str) -> Result<Signer> {
        let (key, _) = SignedSecretKey::from_string(text).context("reading secret key")?;
        Ok(Signer { key })
    }

    pub fn load(path: &Path) -> Result<Signer> {
        let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Self::from_armored(&text)
    }

    pub fn fingerprint(&self) -> String {
        format!("{}", self.key.fingerprint()).to_ascii_uppercase()
    }

    /// ASCII-armored detached signature over `data` (what `repomd.xml.asc` holds).
    pub fn sign_detached(&self, data: &[u8]) -> Result<String> {
        let sig = DetachedSignature::sign_binary_data(
            rand::thread_rng(),
            &self.key.primary_key,
            &Password::empty(),
            HashAlgorithm::Sha256,
            data,
        )
        .context("signing repomd.xml")?;
        sig.to_armored_string(ArmorOptions::default()).context("armoring signature")
    }

    pub fn public_armored(&self) -> Result<String> {
        let public = SignedPublicKey::from(self.key.clone());
        public.to_armored_string(ArmorOptions::default()).context("armoring public key")
    }
}

/// Verify an armored detached signature with an armored public key.
pub fn verify_detached(public_armored: &str, data: &[u8], signature_armored: &str) -> Result<()> {
    let (public, _) = SignedPublicKey::from_string(public_armored).context("reading public key")?;
    let (sig, _) = DetachedSignature::from_string(signature_armored).context("reading signature")?;
    sig.verify(&public.primary_key, data).context("signature does not match")?;
    Ok(())
}

/// Fingerprint of an armored public key, for display.
pub fn public_fingerprint(public_armored: &str) -> Result<String> {
    let (public, _) = SignedPublicKey::from_string(public_armored).context("reading public key")?;
    Ok(format!("{}", public.fingerprint()).to_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_and_verify_round_trip() {
        let g = Signer::generate("quai test <root@localhost>").unwrap();
        let sig = g.signer.sign_detached(b"<repomd/>").unwrap();
        assert!(sig.contains("BEGIN PGP SIGNATURE"));
        verify_detached(&g.public_armored, b"<repomd/>", &sig).unwrap();
        assert!(verify_detached(&g.public_armored, b"<repomd>tampered</repomd>", &sig).is_err());
        let reloaded = Signer::from_armored(&g.secret_armored).unwrap();
        assert_eq!(reloaded.fingerprint(), g.signer.fingerprint());
        assert_eq!(public_fingerprint(&g.public_armored).unwrap(), g.signer.fingerprint());
    }
}
