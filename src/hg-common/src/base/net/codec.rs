use std::{
    fmt,
    ops::{Deref, DerefMut},
};

use bytes::{Buf, BufMut, Bytes, BytesMut};
use tokio_util::codec::Decoder;
use varuint::{Deserializable as _, Serializable as _, Varint};

use super::RpcPacket;

// === Encoder === //

pub struct FrameEncoder {
    header: BytesMut,
    data: BytesMut,
}

impl fmt::Debug for FrameEncoder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.data.fmt(f)
    }
}

impl Default for FrameEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for FrameEncoder {
    fn clone(&self) -> Self {
        let mut clone = Self::new();
        clone.extend_from_slice(&self[..]);
        clone
    }
}

impl FrameEncoder {
    pub fn new() -> Self {
        let mut data = BytesMut::new();
        data.put_bytes(0u8, Varint(isize::MAX as u64).size_hint());
        let header = data.split();

        Self { header, data }
    }

    pub fn single(packet: &impl RpcPacket) -> Bytes {
        let mut encoder = Self::new();
        packet.encode(&mut encoder);
        encoder.finish()
    }

    pub fn finish(mut self) -> Bytes {
        // Write header
        let packet_len = Varint(self.data.len() as u64);
        let header_len = packet_len.size_hint();
        let header_start = self.header.len() - header_len;
        let mut packet_len_buf = &mut self.header[header_start..];
        packet_len.serialize(&mut packet_len_buf).unwrap();

        // Recombine parts
        self.header.unsplit(self.data);

        // Produce final packet
        self.header.advance(header_start);
        self.header.freeze()
    }
}

impl Deref for FrameEncoder {
    type Target = BytesMut;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl DerefMut for FrameEncoder {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

// === Decoder === //

#[derive(Debug)]
pub struct FrameDecoder {
    pub max_packet_size: usize,
}

impl Decoder for FrameDecoder {
    type Item = Bytes;
    type Error = anyhow::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // Decode header
        let mut cursor = &src[..];
        let Ok(Varint(packet_len)) = Varint::<u64>::deserialize(&mut cursor) else {
            return Ok(None);
        };

        let header_len = src.len() - cursor.len();

        let Some(packet_len) = usize::try_from(packet_len)
            .ok()
            .filter(|&v| v <= self.max_packet_size)
        else {
            anyhow::bail!(
                "packet is too large ({packet_len} > {})",
                self.max_packet_size
            );
        };

        // Decode body
        if cursor.len() < packet_len {
            return Ok(None);
        }

        src.advance(header_len);

        Ok(Some(src.split_to(packet_len).freeze()))
    }
}
