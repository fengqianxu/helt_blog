use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit},
};
use rand::random;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgPool};

#[derive(Clone)]
pub struct LlmKeyring {
    current: VersionedKey,
    previous: Option<VersionedKey>,
}

#[derive(Clone)]
struct VersionedKey {
    version: i32,
    key: [u8; 32],
}

#[derive(Debug)]
pub struct EncryptedSecret {
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
    pub key_version: i32,
}

#[derive(Debug, thiserror::Error)]
pub enum LlmCryptoError {
    #[error("LLM encryption key version must be positive")]
    InvalidKeyVersion,
    #[error("current and previous LLM encryption key versions must differ")]
    DuplicateKeyVersion,
    #[error("LLM API key encryption failed")]
    EncryptionFailed,
    #[error("LLM API key columns are incomplete")]
    IncompleteCiphertext,
    #[error("LLM API key nonce is invalid")]
    InvalidNonce,
    #[error("LLM API key uses unavailable encryption key version {0}")]
    UnknownKeyVersion(i32),
    #[error("LLM API key decryption failed")]
    DecryptionFailed,
    #[error("LLM API key plaintext is not UTF-8")]
    InvalidPlaintext,
}

#[derive(Debug, FromRow)]
struct EncryptedConnectionRow {
    id: i64,
    api_key_ciphertext: Vec<u8>,
    api_key_nonce: Vec<u8>,
    encryption_key_version: i32,
}

impl LlmKeyring {
    pub fn new(
        current_version: i32,
        current_secret: &str,
        previous: Option<(i32, &str)>,
    ) -> Result<Self, LlmCryptoError> {
        if current_version <= 0 || previous.is_some_and(|(version, _)| version <= 0) {
            return Err(LlmCryptoError::InvalidKeyVersion);
        }
        if previous.is_some_and(|(version, _)| version == current_version) {
            return Err(LlmCryptoError::DuplicateKeyVersion);
        }

        Ok(Self {
            current: VersionedKey::derive(current_version, current_secret),
            previous: previous.map(|(version, secret)| VersionedKey::derive(version, secret)),
        })
    }

    pub fn current_version(&self) -> i32 {
        self.current.version
    }

    pub fn previous_version(&self) -> Option<i32> {
        self.previous.as_ref().map(|key| key.version)
    }

    pub fn encrypt(&self, plaintext: &str) -> Result<EncryptedSecret, LlmCryptoError> {
        encrypt_with_key(&self.current, plaintext)
    }

    pub fn decrypt_optional(
        &self,
        key_version: Option<i32>,
        ciphertext: Option<&[u8]>,
        nonce: Option<&[u8]>,
    ) -> Result<Option<String>, LlmCryptoError> {
        match (key_version, ciphertext, nonce) {
            (None, None, None) => Ok(None),
            (Some(version), Some(ciphertext), Some(nonce)) => {
                self.decrypt(version, ciphertext, nonce).map(Some)
            }
            _ => Err(LlmCryptoError::IncompleteCiphertext),
        }
    }

    pub fn decrypt(
        &self,
        key_version: i32,
        ciphertext: &[u8],
        nonce: &[u8],
    ) -> Result<String, LlmCryptoError> {
        let key = if self.current.version == key_version {
            &self.current
        } else if self
            .previous
            .as_ref()
            .is_some_and(|key| key.version == key_version)
        {
            self.previous
                .as_ref()
                .expect("previous key version checked above")
        } else {
            return Err(LlmCryptoError::UnknownKeyVersion(key_version));
        };
        decrypt_with_key(key, ciphertext, nonce)
    }
}

impl VersionedKey {
    fn derive(version: i32, secret: &str) -> Self {
        Self {
            version,
            key: Sha256::digest(secret.as_bytes()).into(),
        }
    }
}

fn encrypt_with_key(
    key: &VersionedKey,
    plaintext: &str,
) -> Result<EncryptedSecret, LlmCryptoError> {
    let cipher = XChaCha20Poly1305::new((&key.key).into());
    let nonce_bytes: [u8; 24] = random();
    let ciphertext = cipher
        .encrypt(XNonce::from_slice(&nonce_bytes), plaintext.as_bytes())
        .map_err(|_| LlmCryptoError::EncryptionFailed)?;
    Ok(EncryptedSecret {
        ciphertext,
        nonce: nonce_bytes.to_vec(),
        key_version: key.version,
    })
}

fn decrypt_with_key(
    key: &VersionedKey,
    ciphertext: &[u8],
    nonce: &[u8],
) -> Result<String, LlmCryptoError> {
    if nonce.len() != 24 {
        return Err(LlmCryptoError::InvalidNonce);
    }
    let cipher = XChaCha20Poly1305::new((&key.key).into());
    let plaintext = cipher
        .decrypt(XNonce::from_slice(nonce), ciphertext)
        .map_err(|_| LlmCryptoError::DecryptionFailed)?;
    String::from_utf8(plaintext).map_err(|_| LlmCryptoError::InvalidPlaintext)
}

pub async fn rotate_llm_encryption_keys(
    pool: &PgPool,
    keyring: &LlmKeyring,
) -> anyhow::Result<u64> {
    let previous_version = keyring.previous_version().ok_or_else(|| {
        anyhow::anyhow!(
            "LLM_ENCRYPTION_PREVIOUS_KEY and LLM_ENCRYPTION_PREVIOUS_KEY_VERSION are required"
        )
    })?;
    let mut transaction = pool.begin().await?;
    let rows = sqlx::query_as::<_, EncryptedConnectionRow>(
        "SELECT id, api_key_ciphertext, api_key_nonce, encryption_key_version
         FROM llm_connections
         WHERE api_key_ciphertext IS NOT NULL
         ORDER BY id
         FOR UPDATE",
    )
    .fetch_all(&mut *transaction)
    .await?;

    let mut pending = Vec::new();
    for row in rows {
        if row.encryption_key_version == keyring.current_version() {
            continue;
        }
        if row.encryption_key_version != previous_version {
            anyhow::bail!(
                "connection {} uses unsupported encryption key version {}; configured versions are {} and {}",
                row.id,
                row.encryption_key_version,
                keyring.current_version(),
                previous_version
            );
        }
        let plaintext = keyring.decrypt(
            row.encryption_key_version,
            &row.api_key_ciphertext,
            &row.api_key_nonce,
        )?;
        pending.push((row.id, keyring.encrypt(&plaintext)?));
    }

    for (id, encrypted) in &pending {
        sqlx::query(
            "UPDATE llm_connections
             SET api_key_ciphertext = $1, api_key_nonce = $2, encryption_key_version = $3
             WHERE id = $4",
        )
        .bind(&encrypted.ciphertext)
        .bind(&encrypted.nonce)
        .bind(encrypted.key_version)
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(pending.len() as u64)
}

#[cfg(test)]
mod tests {
    use super::{LlmCryptoError, LlmKeyring};

    #[test]
    fn keyring_encrypts_with_current_and_decrypts_previous_version() {
        let previous = LlmKeyring::new(1, "previous-secret", None).expect("previous key");
        let legacy = previous.encrypt("secret-key").expect("legacy ciphertext");
        let rotating =
            LlmKeyring::new(2, "current-secret", Some((1, "previous-secret"))).expect("keyring");

        assert_eq!(rotating.current_version(), 2);
        assert_eq!(
            rotating
                .decrypt(legacy.key_version, &legacy.ciphertext, &legacy.nonce)
                .expect("decrypt previous"),
            "secret-key"
        );
        assert_eq!(
            rotating
                .encrypt("new-key")
                .expect("encrypt current")
                .key_version,
            2
        );
    }

    #[test]
    fn keyring_rejects_unknown_versions() {
        let keyring = LlmKeyring::new(2, "current-secret", None).expect("keyring");
        let error = keyring
            .decrypt(1, b"ciphertext", &[0; 24])
            .expect_err("version must be rejected");
        assert!(matches!(error, LlmCryptoError::UnknownKeyVersion(1)));
    }
}
