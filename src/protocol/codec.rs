use std::time::SystemTime;

use bytes::{BytesMut, BufMut, Buf};
use futures::StreamExt;
use packed_struct::PackedStruct;
use thiserror::Error;
use tokio_util::codec::{Decoder, Encoder};

use super::commands::Command;



const FRAME_START: u8 = 0x32;
const FRAME_END: u8 = 0x34;

#[derive(Copy, Clone, Debug)]
pub struct FrameId {
    /// Source address
    pub src: u8,

    /// Destination address
    pub dst: u8,

    /// Command
    pub cmd: u8
}

impl From<FrameId> for (u8,u8,u8) {
    fn from(id: FrameId) -> Self {
        (id.src, id.dst, id.cmd)
    }
}


pub trait Unpack<const S: usize> {
    fn unpack_as<T>(&self) -> Result<T, packed_struct::PackingError> where
        T: PackedStruct<ByteArray = [u8; S]> + Command;
}


#[derive(Clone, Debug)]
pub struct FrameInner<const S: usize> {
    pub id: FrameId,
    pub data: [u8; S]
}

impl<const S: usize> FrameInner<S> {
    const DATA_SIZE: usize = S;
    const SIZE: usize = 6 + FrameInner::<S>::DATA_SIZE; // start, src, dst, cmd, data[S], checksum, end

    pub fn new(src: u8, dst: u8, cmd: u8, data: [u8; S]) -> Self {
        Self {
            id: FrameId {
                src, dst, cmd
            },
            data
        }
    }

    /// Pack a `Command` `PackedStruct` into a new `FrameInner` of the appropriate size.
    pub fn pack<T>(src: u8, dst: u8, cmd: T) -> Result<Self, packed_struct::PackingError> where
        T: PackedStruct<ByteArray = [u8; S]> + Command
    {
        cmd.check();

        Ok(Self {
            id: FrameId { src, dst, cmd: T::ID },
            data: cmd.pack()?
        })
    } 

    /// Unpack `Self` as a `Command` `PackedStruct`
    pub fn unpack_as<T>(&self) -> Result<T, packed_struct::PackingError> where
        T: PackedStruct<ByteArray = [u8; S]> + Command
    {
        let cmd = T::unpack(&self.data)?;
        cmd.check();

        Ok(cmd)
    }
}

pub type LongFrame = FrameInner<8>;
pub type ShortFrame = FrameInner<1>;


/// A received frame from a port
#[derive(Clone, Debug)]
pub enum RxFrame {
    Long(LongFrame),
    Short(ShortFrame),
    Corrupted(Vec<u8>)
}

impl RxFrame {
    /// `FrameId` accessor helper
    pub fn id(&self) -> Option<FrameId> {
        match self {
            RxFrame::Long(frame) => Some(frame.id),
            RxFrame::Short(frame) => Some(frame.id),
            RxFrame::Corrupted(_) => None,
        }
    }
}

impl Unpack<{LongFrame::DATA_SIZE}> for RxFrame {
    fn unpack_as<T>(&self) -> Result<T, packed_struct::PackingError> where
        T: PackedStruct<ByteArray = [u8; LongFrame::DATA_SIZE]> + Command {
        if let RxFrame::Long(frame) = self {
            frame.unpack_as::<T>()
        } else {
            panic!("tried to unpack")
        }
    }
}

impl Unpack<{ShortFrame::DATA_SIZE}> for RxFrame {
    fn unpack_as<T>(&self) -> Result<T, packed_struct::PackingError> where
        T: PackedStruct<ByteArray = [u8; ShortFrame::DATA_SIZE]> + Command {
        if let RxFrame::Short(frame) = self {
            frame.unpack_as::<T>()
        } else {
            panic!("tried to unpack")
        }
    }
}

impl From<TxFrame> for RxFrame {
    fn from(frame: TxFrame) -> Self {
        match frame {
            TxFrame::Long(frame) => RxFrame::Long(frame.clone()),
            TxFrame::Short(frame) => RxFrame::Short(frame.clone()),
        }
    }
}


/// A frame to send to a port
#[derive(Clone, Debug)]
pub enum TxFrame {
    Long(LongFrame),
    Short(ShortFrame)
}

impl From<ShortFrame> for TxFrame {
    fn from(value: ShortFrame) -> Self {
        TxFrame::Short(value)
    }
}

impl From<LongFrame> for TxFrame {
    fn from(value: LongFrame) -> Self {
        TxFrame::Long(value)
    }
}

trait Checksum {
    fn checksum(&mut self) -> u8;
}

impl <'a>Checksum for std::slice::Iter<'a, u8> {
    fn checksum(&mut self) -> u8 {
        self.fold(0, |acc, byte| acc ^ byte)
    }
}


#[derive(Error, Debug)]
enum FramingError {
    #[error("input buffer too small")]
    BufferTooSmall,
    #[error("start of frame marker not found")]
    FrameStartNotFound,
    #[error("end of frame marker not found")]
    FrameEndNotFound,
    #[error("invalid checksum (expected {expected:x}, actual: {actual:x}) ")] // for frame {frame:x?}
    InvalidChecksum {
        expected: u8,
        actual: u8,
        //frame: Vec<u8>
    },
}



pub struct WrcBusProtocolCodec {
    start_time: SystemTime,
    last_rx_time: Option<SystemTime>,
    last_tx_time: Option<SystemTime>,
    last_txrx_time: Option<SystemTime>
}

impl WrcBusProtocolCodec {
    pub fn new() -> Self {
        println!("XX  elapsed tx-delta rx-delta    delta");
        WrcBusProtocolCodec {
            start_time: SystemTime::now(),
            last_rx_time: None,
            last_tx_time: None,
            last_txrx_time: None
        }
    }
}

pub fn delta_ms(time: Option<SystemTime>) -> u128 {
    if let Some(time) = time {
        time.elapsed().expect("elapsed time").as_millis()
    } else {
        0
    }
}

impl Decoder for WrcBusProtocolCodec {
    type Item = RxFrame;

    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        fn try_read_frame<const S: usize>(src: &mut BytesMut, codec: &WrcBusProtocolCodec) -> Result<FrameInner<S>, FramingError> {
            let size = FrameInner::<S>::SIZE;

            if src.len() < size {
                return Err(FramingError::BufferTooSmall);
            }

            let end_idx = size - 1;

            if src[0] != FRAME_START {
                return Err(FramingError::FrameStartNotFound)
            }

            if src[end_idx] != FRAME_END {
                return Err(FramingError::FrameEndNotFound)
            }

            let mut src = src.split_to(size);

            let start_delta_ms = delta_ms(Some(codec.start_time));
            let rx_delta_ms = delta_ms(codec.last_rx_time);
            let txrx_delta_ms = delta_ms(codec.last_txrx_time);

            //println!("RX {start_delta_ms:8}          {rx_delta_ms:8} {txrx_delta_ms:8}: {:x?}", &src[..]);

            src.advance(1); // ignore frame start

            let frame = {
                let mut footer = src.split_off(src.len()-2);

                let checksum = footer.get_u8();

                let expected_checksum = src.iter().checksum();
                if expected_checksum != checksum {
                    return Err(FramingError::InvalidChecksum { expected: expected_checksum, actual: checksum })
                }

                FrameInner::<S> {
                    id: FrameId {
                        src: src.get_u8(),
                        dst: src.get_u8(),
                        cmd: src.get_u8()
                    },
                    data: src[..].try_into().unwrap(),
                }
            };

            Ok(frame)
        }

        loop {
            let frame = match try_read_frame(src, self) {
                Ok(frame) => Ok(Some(RxFrame::Short(frame))),

                Err(FramingError::FrameEndNotFound) => {
                    try_read_frame(src, self)
                        .map(|frame| Some(RxFrame::Long(frame)))
                }

                Err(err) => Err(err),
            };

            let frame = match frame {
                Ok(frame) => {
                    self.last_rx_time = Some(SystemTime::now());
                    self.last_txrx_time = Some(SystemTime::now());

                    Ok(frame)
                }

                // need more data to read a short (or long) frame
                Err(FramingError::BufferTooSmall) => return Ok(None),

                Err(FramingError::FrameStartNotFound) | Err(FramingError::FrameEndNotFound) => {
                    // frame start marker not found at beginning of buffer -- skip the junk to resync,
                    // or, frame start marker found that doesn't have a corresponding end frame marker -- skip it
                    //      (likely the start frame marker was inside a previous packet and the stream has lost sync)

                    // TODO: implement a limit on the amount of data we skip -- after a while the stream should be considered
                    // unreadable and be an Error
                    src.advance(1);
                    continue;
                }

                // TODO: handle checksum errors "gracefully"
                // maybe only bail out after n checksum errors?
                Err(FramingError::InvalidChecksum { expected, actual }) => {
                    todo!();
                    Ok(Some(RxFrame::Corrupted(vec![])))
                }

            };

            return frame
        }
    }
}

impl Encoder<TxFrame> for WrcBusProtocolCodec {
    type Error = std::io::Error;

    fn encode(&mut self, frame: TxFrame, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let (header, data) = match &frame {
            TxFrame::Long(frame) => {
                dst.reserve(LongFrame::SIZE);
                (&frame.id, &frame.data[..])
            },
            TxFrame::Short(frame) => {
                dst.reserve(ShortFrame::SIZE);
                (&frame.id, &frame.data[..])
            },
        };

        dst.put_u8(FRAME_START);
        dst.put_u8(header.src);
        dst.put_u8(header.dst);
        dst.put_u8(header.cmd);

        dst.put(data);

        let checksum = dst[1..dst.len()].iter().checksum();
        dst.put_u8(checksum);
        dst.put_u8(FRAME_END);

        let start_delta_ms = delta_ms(Some(self.start_time));
        let tx_delta_ms = delta_ms(self.last_tx_time);
        let txrx_delta_ms = delta_ms(self.last_txrx_time);

        //println!("TX {start_delta_ms:8} {tx_delta_ms:8}          {txrx_delta_ms:8}: {:x?}", &dst[..]);

        self.last_tx_time = Some(SystemTime::now());
        self.last_txrx_time = Some(SystemTime::now());

        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use tokio_util::codec::FramedRead;

    use super::*;

    #[tokio::test]
    async fn test_codec_decode() {
        let valid_short_frame = [0x32, 0x84, 0xeb, 0xf9, 0x00, 0x96, 0x34];

        let mut fr = FramedRead::new(&valid_short_frame[..], WrcBusProtocolCodec::new());
        let frame = fr.next().await.unwrap().unwrap();
        println!("{frame:?}");

        let valid_long_frame = [0x32, 0x20, 0x84, 0x52, 0x4b, 0x4c, 0x63, 0xf8, 0x81, 0x10, 0x0, 0x6f, 0x94, 0x34];

        let mut fr = FramedRead::new(&valid_short_frame[..], WrcBusProtocolCodec::new());
        let frame = fr.next().await.unwrap().unwrap();


        {
            let mut buf = BytesMut::new();
            buf.put(&[0x01, 0x02, 0x32, 0x01, 0x02, 0x32][..]); // some junk which includes a start-of-frame marker
            buf.put(&valid_long_frame[..]);
            println!("{:x?}", &buf[..]);

            let mut fr = FramedRead::new(&buf[..], WrcBusProtocolCodec::new());
            let frame = fr.next().await.unwrap().unwrap();
        }



        // two short frames (looks like 1 invalid long frame)
        {
            let mut buf = BytesMut::new();
            buf.put(&valid_short_frame[..]);
            buf.put(&valid_short_frame[..]);

            let mut fr = FramedRead::new(&buf[..], WrcBusProtocolCodec::new());
            let frame = fr.next().await.unwrap().unwrap();
            let frame = fr.next().await.unwrap().unwrap();
        }

        // long frame with nested invalid short frame
        {

        }

        // long frame with nested valid short frame
        {
            let mut ambiguious_frame = [0x32, 0x01, 0x02, 0xAA, 0x04, 0xFF, 0x34,  0x32, 0x01, 0x02, 0xBB, 0x00, 0xFF, 0x34];
            ambiguious_frame[5] = ambiguious_frame[1..5].iter().checksum();
            ambiguious_frame[12] = ambiguious_frame[1..12].iter().checksum();


        }
    }

    #[test]
    fn test_codec_encode() {
        let mut codec = WrcBusProtocolCodec::new();


        let mut buf = BytesMut::new();
        let frame = TxFrame::Short(ShortFrame::new(0x1, 0x2, 0x3, [0x4]));
        codec.encode(frame, &mut buf).expect("encode");
    }
}