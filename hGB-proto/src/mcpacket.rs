use crate::error::TunnelError;
use crate::varint::{read_varint, read_varint_from_bytes, write_varint};
use bytes::Bytes;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub async fn read_packet<R: AsyncRead + Unpin>(
    reader: &mut R,
) -> Result<(i32, Bytes), TunnelError> {
    let length = read_varint(reader).await? as usize;
    let mut raw = vec![0u8; length];
    reader.read_exact(&mut raw).await?;
    let (pid, offset) = read_varint_from_bytes(&raw)?;
    Ok((pid, Bytes::from(raw).slice(offset..)))
}

pub async fn send_packet<W: AsyncWrite + Unpin>(
    writer: &mut W,
    packet_id: i32,
    payload: &[u8],
) -> Result<(), TunnelError> {
    let mut body = write_varint(packet_id);
    body.extend_from_slice(payload);
    let mut out = write_varint(body.len() as i32);
    out.extend_from_slice(&body);
    writer.write_all(&out).await?;
    writer.flush().await?;
    Ok(())
}
