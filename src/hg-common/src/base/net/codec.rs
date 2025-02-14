use bytes::{Buf, BufMut, Bytes, BytesMut};
use serde::Serialize;
use tokio_util::codec::{Decoder, Encoder};
use varuint::{Deserializable as _, Serializable as _, Varint};

use crate::utils::lang::ExtendMutAdapter;

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

#[derive(Debug)]
pub struct FrameEncoder;

impl<T: Serialize> Encoder<T> for FrameEncoder {
    type Error = anyhow::Error;

    fn encode(&mut self, item: T, dst: &mut BytesMut) -> Result<(), Self::Error> {
        // Reserve space for a maximally-large header.
        // We can't go beyond `isize::MAX` since allocations are guaranteed to be below this size.
        let max_header_len = Varint(isize::MAX as u64).size_hint();
        dst.put_bytes(0u8, max_header_len);

        // Encode the packet
        let packet_start = dst.len();
        postcard::to_extend(&item, ExtendMutAdapter(dst))?;

        // Write the header.
        let packet_len = Varint((dst.len() - packet_start) as u64);
        let header_len = packet_len.size_hint();
        let mut packet_len_buf = &mut dst[(packet_start - header_len)..packet_start];
        packet_len.serialize(&mut packet_len_buf).unwrap();

        // Truncate the unused header space.
        dst.advance(max_header_len - header_len);

        Ok(())
    }
}
