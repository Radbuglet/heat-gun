use std::fmt;

use anyhow::Context as _;
use bytes::{Bytes, BytesMut};
use serde::{de::DeserializeOwned, Serialize};
use varuint::{Deserializable, Serializable, Varint};

use crate::utils::lang::{ExtendMutAdapter, Extends};

const MAX_VAR_INT_LEN: usize = 9;

// === RpcPacket === //

pub trait RpcPacket:
    'static + Sized + Send + Sync + Clone + fmt::Debug + Serialize + DeserializeOwned
{
    fn decode(data: &[u8]) -> anyhow::Result<Self>;

    fn encode(&self, out: &mut BytesMut);
}

impl<T> RpcPacket for T
where
    T: 'static + Sized + Send + Sync + Clone + fmt::Debug + Serialize + DeserializeOwned,
{
    fn decode(data: &[u8]) -> anyhow::Result<Self> {
        postcard::from_bytes(data).context("failed to deserialize packet")
    }

    fn encode(&self, out: &mut BytesMut) {
        postcard::to_extend(self, ExtendMutAdapter(out)).unwrap();
    }
}

// === MultiPart Serialization === //

pub trait MultiPartSerializeExt: Extends<BytesMut> {
    fn encode_multi_part_raw<R>(&mut self, writer: impl FnOnce(&mut BytesMut) -> R) -> R;

    fn encode_multi_part(&mut self, packet: &impl RpcPacket);
}

impl MultiPartSerializeExt for BytesMut {
    fn encode_multi_part_raw<R>(&mut self, writer: impl FnOnce(&mut BytesMut) -> R) -> R {
        // Encode the packet
        let start = self.len();
        let res = writer(self);
        let len = self.len() - start;

        // Encode the length backwards
        let mut len_bytes = [0u8; MAX_VAR_INT_LEN];
        let len_size = Varint::<u64>(len as u64)
            .serialize(&mut &mut len_bytes[..])
            .unwrap();

        let len_bytes = &mut len_bytes[..len_size];
        len_bytes.reverse();
        self.extend_from_slice(&len_bytes);

        res
    }

    fn encode_multi_part(&mut self, packet: &impl RpcPacket) {
        self.encode_multi_part_raw(|data| packet.encode(data));
    }
}

// === MultiPart Deserialization === //

#[derive(Debug, Clone)]
pub struct MultiPartDecoder {
    remaining: Bytes,
}

impl MultiPartDecoder {
    pub fn new(packet: Bytes) -> Self {
        Self { remaining: packet }
    }

    pub fn remaining(&self) -> &Bytes {
        &self.remaining
    }

    pub fn expect(&mut self) -> anyhow::Result<Bytes> {
        self.next().context("unexpected end of multi-part packet")?
    }

    pub fn expect_rich<T: RpcPacket>(&mut self) -> anyhow::Result<T> {
        T::decode(&self.expect()?)
    }
}

impl Iterator for MultiPartDecoder {
    type Item = anyhow::Result<Bytes>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining.is_empty() {
            return None;
        }

        // Read the last few bytes of the buffer in reverse order.
        let mut len_bytes = [0u8; MAX_VAR_INT_LEN];
        let len_bytes_src = &self.remaining[self.remaining.len().saturating_sub(MAX_VAR_INT_LEN)..];
        len_bytes[..len_bytes_src.len()].copy_from_slice(len_bytes_src);
        let len_bytes = &mut len_bytes[..len_bytes_src.len()];
        len_bytes.reverse();

        // Parse the length encoded by the backwards varint.
        let mut len_bytes_cursor = &*len_bytes;
        let Varint(part_len) = match Varint::<u64>::deserialize(&mut len_bytes_cursor) {
            Ok(len) => len,
            Err(e) => {
                return Some(Err(
                    anyhow::Error::from(e).context("invalid multi-part length")
                ))
            }
        };
        let footer_len = len_bytes.len() - len_bytes_cursor.len();
        self.remaining.truncate(self.remaining.len() - footer_len);

        // Extract it from the buffer.
        let Some(new_remaining_len) = usize::try_from(part_len)
            .ok()
            .and_then(|v| self.remaining.len().checked_sub(v))
        else {
            return Some(Err(anyhow::anyhow!(
                "part has length {part_len} but remaining buffer has length {}",
                self.remaining.len()
            )));
        };

        let data = self.remaining.split_off(new_remaining_len);

        Some(Ok(data))
    }
}

// === Tests === //

#[cfg(test)]
mod tests {
    use bytes::{BufMut, BytesMut};

    use super::{MultiPartDecoder, MultiPartSerializeExt as _};

    #[test]
    fn round_trip() {
        let mut buf = BytesMut::new();

        buf.encode_multi_part_raw(|_buf| {});

        buf.encode_multi_part_raw(|buf| {
            buf.put_bytes(42, 100);
        });

        buf.encode_multi_part_raw(|_buf| {});

        buf.encode_multi_part_raw(|buf| {
            buf.put_bytes(42, 10);
        });

        let mut dec = MultiPartDecoder::new(buf.freeze());

        assert_eq!(10, dec.next().unwrap().unwrap().len());
        assert_eq!(0, dec.next().unwrap().unwrap().len());
        assert_eq!(100, dec.next().unwrap().unwrap().len());
        assert_eq!(0, dec.next().unwrap().unwrap().len());
        assert!(dec.next().is_none());
    }
}
