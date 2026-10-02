pub mod crypto;
pub mod error;
pub mod mcpacket;
pub mod varint;

pub use crypto::{
    derive_key, read_chunk, relay_encrypted_to_plain, relay_plain_to_encrypted, write_chunk,
    NonceCounter, KEY_LEN, MAX_CHUNK_PLAINTEXT, SALT_LEN,
};
pub use error::TunnelError;
pub use mcpacket::{read_packet, send_packet};
pub use varint::{
    read_string, read_string_from_bytes, read_varint, read_varint_from_bytes, write_string,
    write_varint,
};
