use thiserror::Error;

#[derive(Debug, Error)]
pub enum TunnelError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("decrypt failed")]
    DecryptFailed,

    #[error("nonce space exhausted, connection need to recreate")]
    NonceExhausted,

    #[error("chunk too large: {0} > max {1}")]
    ChunkTooLarge(usize, usize),

    #[error("varint too long")]
    VarIntTooLong,

    #[error("malformed packet: {0}")]
    Malformed(&'static str),

    #[error("peer closed connection")]
    PeerClosed,
}

impl TunnelError {
    pub fn is_benign_close(&self) -> bool {
        match self {
            TunnelError::Io(e) => matches!(
                e.kind(),
                std::io::ErrorKind::UnexpectedEof
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::ConnectionAborted
            ),
            TunnelError::PeerClosed => true,
            _ => false,
        }
    }
}
