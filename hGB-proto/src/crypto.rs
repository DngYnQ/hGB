use crate::error::TunnelError;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const SALT_LEN: usize = 16;
pub const KEY_LEN: usize = 32;
pub const MAX_CHUNK_PLAINTEXT: usize = 16384;

pub struct NonceCounter {
    counter: u128,
}

impl NonceCounter {
    pub fn new() -> Self {
        Self { counter: 0 }
    }

    pub fn next(&mut self) -> Result<[u8; 12], TunnelError> {
        if self.counter >= (1u128 << 96) - 1 {
            return Err(TunnelError::NonceExhausted);
        }
        let c = self.counter;
        self.counter += 1;
        let full = c.to_be_bytes();
        let mut nonce = [0u8; 12];
        nonce.copy_from_slice(&full[4..16]);
        Ok(nonce)
    }
}

impl Default for NonceCounter {
    fn default() -> Self {
        Self::new()
    }
}

pub fn derive_key(secret: &[u8], salt: &[u8], info: &[u8]) -> [u8; KEY_LEN] {
    let hk = Hkdf::<Sha256>::new(Some(salt), secret);
    let mut okm = [0u8; KEY_LEN];
    hk.expand(info, &mut okm)
        .expect("HKDF expand for a 12‑byte key cannot fail");
    okm
}

pub async fn write_chunk<W: AsyncWrite + Unpin>(
    writer: &mut W,
    key: &[u8; KEY_LEN],
    nonces: &mut NonceCounter,
    plaintext: &[u8],
) -> Result<(), TunnelError> {
    if plaintext.len() > MAX_CHUNK_PLAINTEXT {
        return Err(TunnelError::ChunkTooLarge(plaintext.len(), MAX_CHUNK_PLAINTEXT));
    }
    let cipher = Aes256Gcm::new_from_slice(key).expect("key always 32 bytes");
    let nonce_bytes = nonces.next()?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, plaintext)
        .map_err(|_| TunnelError::DecryptFailed)?;
    writer.write_all(&(ct.len() as u16).to_be_bytes()).await?;
    writer.write_all(&ct).await?;
    writer.flush().await?;
    Ok(())
}

pub async fn read_chunk<R: AsyncRead + Unpin>(
    reader: &mut R,
    key: &[u8; KEY_LEN],
    nonces: &mut NonceCounter,
) -> Result<Vec<u8>, TunnelError> {
    let mut len_buf = [0u8; 2];
    reader.read_exact(&mut len_buf).await?;
    let ct_len = u16::from_be_bytes(len_buf) as usize;
    let mut ct = vec![0u8; ct_len];
    reader.read_exact(&mut ct).await?;
    let cipher = Aes256Gcm::new_from_slice(key).expect("key always 32 bytes");
    let nonce_bytes = nonces.next()?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    cipher
        .decrypt(nonce, ct.as_ref())
        .map_err(|_| TunnelError::DecryptFailed)
}

pub async fn relay_encrypted_to_plain<R, W>(
    mut enc_reader: R,
    key: [u8; KEY_LEN],
    mut nonces: NonceCounter,
    mut plain_writer: W,
) -> Result<(), TunnelError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let result = loop {
        match read_chunk(&mut enc_reader, &key, &mut nonces).await {
            Ok(data) => {
                if data.is_empty() {
                    continue;
                }
                if let Err(e) = plain_writer.write_all(&data).await {
                    break Err(e.into());
                }
                if let Err(e) = plain_writer.flush().await {
                    break Err(e.into());
                }
            }
            Err(e) => break Err(e),
        }
    };
    let _ = plain_writer.shutdown().await;
    result
}

pub async fn relay_plain_to_encrypted<R, W>(
    mut plain_reader: R,
    mut enc_writer: W,
    key: [u8; KEY_LEN],
    mut nonces: NonceCounter,
) -> Result<(), TunnelError>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buf = vec![0u8; MAX_CHUNK_PLAINTEXT];
    let result = loop {
        match plain_reader.read(&mut buf).await {
            Ok(0) => break Ok(()),
            Ok(n) => {
                if let Err(e) = write_chunk(&mut enc_writer, &key, &mut nonces, &buf[..n]).await {
                    break Err(e);
                }
            }
            Err(e) => break Err(e.into()),
        }
    };
    let _ = enc_writer.shutdown().await;
    result
}

