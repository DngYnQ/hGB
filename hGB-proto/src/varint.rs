use crate::error::TunnelError;
use tokio::io::{AsyncRead, AsyncReadExt};

pub async fn read_varint<R: AsyncRead + Unpin>(reader: &mut R) -> Result<i32, TunnelError> {
    let mut value: i32 = 0;
    for i in 0..5 {
        let mut buf = [0u8; 1];
        reader.read_exact(&mut buf).await?;
        let b = buf[0];
        value |= ((b & 0x7F) as i32) << (7 * i);
        if b & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(TunnelError::VarIntTooLong)
}

pub fn write_varint(value: i32) -> Vec<u8> {
    let mut v = value as u32;
    let mut out = Vec::with_capacity(5);
    loop {
        let mut b = (v & 0x7F) as u8;
        v >>= 7;
        if v != 0 {
            b |= 0x80;
            out.push(b);
        } else {
            out.push(b);
            break;
        }
    }
    out
}

pub fn read_varint_from_bytes(data: &[u8]) -> Result<(i32, usize), TunnelError> {
    let mut value: i32 = 0;
    for i in 0..5 {
        let b = *data.get(i).ok_or(TunnelError::VarIntTooLong)?;
        value |= ((b & 0x7F) as i32) << (7 * i);
        if b & 0x80 == 0 {
            return Ok((value, i + 1));
        }
    }
    Err(TunnelError::VarIntTooLong)
}

pub fn read_string_from_bytes(data: &[u8]) -> Result<(String, usize), TunnelError> {
    let (len, off) = read_varint_from_bytes(data)?;
    let len = len as usize;
    let end = off + len;
    let slice = data
        .get(off..end)
        .ok_or(TunnelError::Malformed("string length out of bounds"))?;
    let s = String::from_utf8(slice.to_vec()).map_err(|_| TunnelError::Malformed("bad utf8"))?;
    Ok((s, end))
}

pub fn write_string(s: &str) -> Vec<u8> {
    let bytes = s.as_bytes();
    let mut out = write_varint(bytes.len() as i32);
    out.extend_from_slice(bytes);
    out
}

#[allow(dead_code)]
pub async fn read_string<R: AsyncRead + Unpin>(reader: &mut R) -> Result<String, TunnelError> {
    let len = read_varint(reader).await? as usize;
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    String::from_utf8(buf).map_err(|_| TunnelError::Malformed("bad utf8"))
}
