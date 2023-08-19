use std::{net::TcpStream, io::{Read, Write}, collections::VecDeque, thread::{self, JoinHandle}, fs::File, time::{SystemTime, Duration}};
use anyhow::{Result, bail};
use thiserror::Error;
use tokio::sync::{broadcast::{Sender, self, Receiver}, mpsc};
use tokio_serial::{SerialPort, SerialPortBuilderExt, SerialStream};






mod packets {
    use std::fmt::Debug;

    use packed_struct::prelude::*;

    pub enum DeviceAddress {
        IndoorUnitBase = 0x20,  // 0x20 - 0x3f
        ErvBase = 0x50, // 0x50 - 0x64

        MainWrc = 0x84,
        SubWrc = 0x85,

        TrackingBroadcast = 0xeb,
        Broadcast = 0xc9,

        // 0xad  (seen packet 0xd1)
    }


    #[derive(PrimitiveEnum, Clone, Copy, Debug)]
    pub enum TemperatureUnit {
        Celsius,
        Fahrenheit
    }

    #[derive(PackedStruct)]
    #[packed_struct(bit_numbering="msb0")]
    pub struct Temperature {
        #[packed_field(bits="0", ty="enum")]
        unit: TemperatureUnit,

        #[packed_field(bits="1:7")]
        raw_value: u8
    }

    impl Temperature {
        fn celsius(&self) -> u8 {
            match self.unit {
                TemperatureUnit::Celsius => self.raw_value - 55,
                TemperatureUnit::Fahrenheit => self.raw_value,
            }
        }
    }

    impl Debug for Temperature {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}C", self.celsius())
        }
    }


    /// Command `0xA0` request packet.
    /// Sent from the WRC to IUs on the bus to change their settings.
    /// IUs reply with command '0x50'.
    #[derive(PackedStruct, Debug)]
    #[packed_struct(bit_numbering="msb0")]
    pub struct ChangeIndoorUnitSettings {
        #[packed_field(bytes="1")]
        unknown: u8,

        #[packed_field(bits="16:18")]
        fan_speed: u8,

        #[packed_field(bits="19:23")]
        setpoint_temp: u8,

        #[packed_field(bits="25")]
        s_plasma_ion: bool,

        #[packed_field(bits="26")]
        reset_clean_filter: bool,

        #[packed_field(bits="27")]
        humidifier: bool,

        #[packed_field(bits="29:31")]
        mode: u8,
    }

    /// Command `0xA2` request packet.
    /// Sent from the WRC to ERVs on the bus to change their settings.
    /// ERVs reply with 
    #[derive(PackedStruct, Debug)]
    #[packed_struct(bit_numbering="msb0")]
    pub struct ChangeErvSettings {
        #[packed_field(bits="16:18")]
        fan_speed: u8, // b000, b100, 101, 111

        #[packed_field(bits="25")]
        clean_up: bool,

        #[packed_field(bits="27:28")]
        other_mode: u8, // b00 = off, b01 = cool, b10 = heat, b11 = auto

        // 34-35: power
        power: u8, // b00 = off, b11 = on (weird, why not just 1 bit?)
    }


    pub enum FanSpeed {
        Auto,
        Low,
        Medium,
        High
    }

    /// Command `0x52` response packet.
    /// Sent from the IU to the requesting WRC on the bus.
    #[derive(PackedStruct, Debug)]
    #[packed_struct(bit_numbering="msb0")]
    pub struct IndoorUnitInfo1 {
        // byte 0

        /// Setpoint Temperature
        #[packed_field(bytes="0")]
        setpoint_temp: Temperature,

        // byte 1

        /// Indoor Unit Room Temperature
        #[packed_field(bytes="1")]
        iu_room_temp: Temperature,

        // byte 2

        /// Indoor Unit Evaporator Inlet Temperature (WRC service menu: 3,2)
        #[packed_field(bytes="2")]
        iu_eva_inlet_temp: Temperature,

        // byte 3

        // Fan Speed 
        #[packed_field(bits="29:31")]
        fan_speed: u8,

        // byte 4

        #[packed_field(bits="32")]
        power: bool,

        #[packed_field(bits="35")]
        defrost: bool,




        //mode: 

        //#[packed_field(bytes="")]
        //unknown: [u8; 3],

        /// Indoor Unit Evaporator Outlet Temperature (WRC service menu: 3,3)
        #[packed_field(bytes="7")]
        iu_eva_outlet_temp: Temperature
    }

    /// Command `0x52` response packet.
    /// Sent from the ERV to the requesting WRC on the bus.
    #[derive(PackedStruct, Debug)]
    #[packed_struct(bit_numbering="msb0")]
    pub struct ErvInfo1 {
        #[packed_field(bits="32")]
        power: bool,

        #[packed_field(bits="33")]
        clean_up: bool,


    }

    /// Packet `0x53`
    #[derive(PackedStruct, Debug)]
    #[packed_struct(bit_numbering="msb0")]
    pub struct IndoorUnitInfo2 {
        #[packed_field(bits="59")]
        humidifier: bool
    }

    /// Packet `0x54`
    #[derive(PackedStruct, Debug)]
    #[packed_struct(bit_numbering="msb0")]
    pub struct IndoorUnitInfo3 {

        /// Outdoor Air Intake
        #[packed_field(bits="56")]
        oa_intake: bool,

        /// Away/MDS (Motion Detect Sensor)
        #[packed_field(bits="57")]
        away_mds: bool,

        #[packed_field(bits="58")]
        unknown58: bool,

        /// S-Plasma Ion
        #[packed_field(bits="59")]
        s_plasma_ion: bool,

        #[packed_field(bits="60")]
        unknown60: bool,

        /// Range Hood (related to ERV)
        #[packed_field(bits="61")]
        range_hood: bool,

        /// Discharge temperature control
        #[packed_field(bits="62")]
        discharge_temp_control: bool,
    }


    /// Packet `0x55`
    #[derive(PackedStruct, Debug)]
    #[packed_struct(bit_numbering="msb0")]
    pub struct IndoorUnitInfo4 {
        #[packed_field(bits="0..=3")]
        humidity: u8,

        #[packed_field(bits="4..=7")]
        co2: u8
    }

    #[derive(PrimitiveEnum, Clone, Copy, Debug)]
    pub enum TemperatureProbeSource {
        IndoorUnitSensor,
        WiredRemoteSensor
    }

    #[derive(PackedStruct)]
    pub struct PrecisionTemperature {
        high: u8,
        low: u8
    }

    impl PrecisionTemperature {
        fn celsius(&self) -> f32 {
            let val = ((self.high as i16) << 8) | (self.low as i16);
            return (val - 553) as f32 / 10.0;
        }
    }

    impl Debug for PrecisionTemperature {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}C", self.celsius())
        }
    }

    // Packet 0x64
    #[derive(PackedStruct, Debug)]
    #[packed_struct(bit_numbering="msb0")]
    pub struct TemperatureProbe {
        #[packed_field(bytes="0")]
        unknown0: u8,

        #[packed_field(bits="8..=14")]
        unknown1: u8,

        #[packed_field(bits="15", ty="enum")]
        active_temp_sensor: TemperatureProbeSource,

        #[packed_field(bytes="2..=3")]
        wrc_temperature: PrecisionTemperature,

        #[packed_field(bytes="4..=5")]
        iu_temperature: PrecisionTemperature,

        unknown2: u8,
        unknown3: u8
    }

    /// Packet `0xc4`
    /// Sent from main WRC `0x84` to sub WRC `0x85`.
    /// Also as a `0xc9` broadcast.
    #[derive(PackedStruct, Debug)]
    #[packed_struct(bit_numbering="msb0")]
    pub struct BusStatus1 {
        /// Unknown. 
        #[packed_field(bytes="0")]
        unknown0: u8,

        #[packed_field(bytes="1")]
        number_of_indoor_units: u8,
    }

    /// Packet `0xc6`
    #[derive(PackedStruct, Debug)]
    #[packed_struct(bit_numbering="msb0")]
    pub struct BusStatus2 {
        /// Unknown. Seems to be always `0x22`
        #[packed_field(bytes="0")]
        unknown0: u8,

        #[packed_field(bytes="1")]
        number_of_indoor_units: u8,
    }



    /// Tracking command `0xfb` packet.
    pub struct TrackingPacketB {
        state: u8 // 2, 1, 0
    }


    pub enum Packet {

    }
}



#[derive(Clone, Debug)]
struct Address {
    src: u8,
    dst: u8
}


#[derive(Clone, Debug)]
struct FrameInner<const S: usize> {
    address: Address,
    command: u8,
    data: [u8; S]
}

impl<const S: usize> FrameInner<S> {
    fn new(src: u8, dst: u8, cmd: u8, data: [u8; S]) -> Self {
        Self {
            address: Address {
                src, dst
            },
            command: cmd,
            data
        }
    }

    fn id(&self) -> (u8, u8, u8) {
        (self.address.src, self.address.dst, self.command)
    }
}

type LongFrame = FrameInner<8>;
type ShortFrame = FrameInner<1>;

#[derive(Clone, Debug)]
enum Frame {
    Long(LongFrame),
    Short(ShortFrame)
}


struct WrcBus {
    port: SerialStream,
}

trait Checksum {
    fn checksum(&mut self) -> u8;
}

impl <'a>Checksum for std::slice::Iter<'a, u8> {
    fn checksum(&mut self) -> u8 {
        self.fold(0, |acc, byte| acc ^ byte)
    }
}

trait Port: Read + Write + Send {}

impl Port for TcpStream {}

// impl Port for SerialPort {}


impl WrcBus {
    fn new(mut port: SerialStream) -> (Self, broadcast::Receiver<Frame>) {

        let (rx_frames_send, rx_frames_recv) = broadcast::channel(16);
        let (tx_frames_send, tx_frames_recv) = mpsc::channel(1);

        tokio::spawn(WrcBus::port_task(port, tx_frames_recv, rx_frames_send));
        //let stream = Arc::new(Mutex::new(stream));
        
        //let (frames_send, frames_recv) = channel();

        //let handle = WrcBus::spawn_reader_thread(stream.try_clone().expect("clone serialport"), frames_send);

        (
            WrcBus {
                port
            },
            rx_frames_recv
        )
    }

    async fn port_task(mut port: SerialStream, tx_frames_recv: mpsc::Receiver<Frame>, rx_frames_send: broadcast::Sender<Frame>) {
        const FRAME_START: u8 = 0x32;
        const FRAME_END: u8 = 0x34;

        #[derive(Error, Debug)]
        enum FramingError {
            #[error("input buffer too small")]
            BufferTooSmall,
            #[error("start of frame marker not found")]
            FrameStartNotFound,
            #[error("invalid checksum (expected {expected}, actual: {actual}) for frame {frame:x?}")]
            InvalidChecksum {
                expected: u8,
                actual: u8,
                frame: Vec<u8>
            }
        }

        impl<const S: usize> FrameInner<S> {
            const SIZE: usize = 6 + S; // start, src, dst, cmd, data[S], checksum, end
        }

        fn try_read_frame<const S: usize>(buffer: &mut VecDeque<u8>, file: &mut File, start: &SystemTime) -> std::result::Result<FrameInner<S>, FramingError> {
            if buffer.len() < FrameInner::<S>::SIZE {
                return Err(FramingError::BufferTooSmall);
            }

            let start_idx = buffer.len() - FrameInner::<S>::SIZE;

            if buffer[start_idx] != FRAME_START {
                return Err(FramingError::FrameStartNotFound)
            }

            let frame = {
                let frame = &buffer.make_contiguous()[start_idx..];

                let ts = start.elapsed().unwrap();
                writeln!(file, "{}: {:x?}", ts.as_millis(), frame).unwrap();

                let [_, data @ .., checksum, _] = frame else {  // discard start/end
                    unreachable!("unpack frame")
                };

                let expected_checksum = data.iter().checksum();
                if expected_checksum != *checksum {
                    return Err(FramingError::InvalidChecksum { expected: expected_checksum, actual: *checksum, frame: frame.into() })
                }

                let [src, dst, cmd, data @ ..] = data else {
                    unreachable!("unpack frame")
                };

                FrameInner::<S> {
                    address: Address { src: *src, dst: *dst },
                    command: *cmd,
                    data: data.try_into().unwrap(),
                }
            };

            buffer.clear();

            Ok(frame)
        }


        let mut file = File::create("standalone-wrc-boot.txt")?;

        let start = SystemTime::now();


        let mut buffer = VecDeque::new();

        

        loop {
            let mut byte = [0; 1];

            let n = port.read(&mut byte).await?;
            if n == 0 {
                bail!("EOF!");
            }

            if buffer.len() > LongFrame::SIZE {
                buffer.pop_front();
            }
            buffer.push_back(byte[0]);

            if buffer[buffer.len() - 1] != FRAME_END {
                continue;
            }

            

            // try to read a long frame, falling back to trying a short frame
            // if there isn't enough data or the frame start wasn't found
            let frame = match try_read_frame(&mut buffer, &mut file, &start) {
                Ok(frame) => Ok(Frame::Long(frame)),
                Err(FramingError::BufferTooSmall) | Err(FramingError::FrameStartNotFound) => {
                    try_read_frame(&mut buffer, &mut file, &start)
                        .map(|frame| Frame::Short(frame))
                },
                Err(err) => Err(err),
            };

            match frame {
                Ok(frame) => frames_send.send(frame).expect("send on frames_send"),
                // can occur if the frame end marker appears inside a frame -- ignore and keep reading 
                Err(FramingError::BufferTooSmall) | Err(FramingError::FrameStartNotFound) => continue,

                Err(err) => {
                    println!("error: {}", err)
                }
            };
        }
        
    }

    pub fn send<const S: usize>(&mut self, frame: &FrameInner<S>) -> Result<()> {
        let frame_start = [FRAME_START, frame.address.src, frame.address.dst, frame.command];

        let checksum = frame_start[1..].iter().checksum() ^ frame.data.iter().checksum();

        let frame_end = [checksum, FRAME_END];

        self.stream.write(&frame_start)?;
        self.stream.write(&frame.data[..])?;
        self.stream.write(&frame_end)?;

        println!("{:x?} {:x?} {:x?}", frame_start, frame.data, frame_end);

        Ok(())
    }
}

fn decode(frame: &Frame) {
    todo!();
    // match frame {
    //     Frame::Long(frame) => match frame.command {
    //         0x
    //     },
    //     Frame::Short(_) => todo!(),
    // }
}

//const LONG_FRAME_LENGTH: usize = 14;
// const SHORT_PACKET_LENGTH: usize = 8;



#[tokio::main]
async fn main() -> Result<()> {
    //let mut stream = TcpStream::connect("192.168.2.104:8899")?;
    let mut port = tokio_serial::new("/dev/ttyUSB0", 2400).timeout(Duration::from_secs(60)).open_native_async()?;

    let (mut bus, frames_recv) = WrcBus::new(port);

    enum State {
        Tracking,
        GeneralOperation
    }

    let mut fb_replies = vec![(0x6, 0xa6), (0xa, 0x6),  (0x0, 0xc)].into_iter();

    let state = State::Tracking;

    for frame in frames_recv.iter() {
        // match state {
        //     State::Tracking => {
        //         match frame
        //     },
        //     State::GeneralOperation => todo!(),
        // }

        println!("recv: {:x?}", frame);

        match frame {
            Frame::Long(frame) => {
                match frame.id() {
                    (0x84, 0xeb, 0xfb) => {
                        let (src, data) = (0x6, 0xa6); //fb_replies.next().expect("next");
                        bus.send(&ShortFrame::new(src, 0x84, 0xfc, [data]))?
                    },
                    other => ()
                }
            },
            Frame::Short(frame) => {
                match frame.id() {
                    (0x84, dst, 0xfd) => bus.send(&ShortFrame::new(dst, 0x84, 0xfe, [0x00]))?,
                    other => ()
                }
            },
        }
    }

    bus.join();

        // long 0xfb data byte 0 goes 2, [1, 0, 1, 0, 1, 0, .. ]

        // fb -> fc
        // fd -> fe

        // match frame {
        //     Ok(Frame::Long(frame)) => {
        //         // if frame.address.src == 0x20 {
        //         //     if frame.command == 0x52 {
        //         //         let packet = packets::IndoorUnitInfo1::unpack(&frame.data)?;
        //         //         println!("{:?}", packet)
        //         //     }
        //         // }

        //         // if frame.command == 0x64 {
        //         //     let packet = packets::TemperatureProbe::unpack(&frame.data)?;
        //         //     println!("{:?}", packet)
        //         // }

        //         // // if frame.address.src == 0x84 {
        //         // //     if 
        //         // // }

        //         // if frame.command == 0x70 || frame.command == 0x71 {
        //         //     println!("{:?}", frame);
        //         // }

        //         let (src, dst, cmd) = (1, 2, 3);

        //         match (src, dst, cmd) {
        //             ( 1, 2, 3 ) => {}
        //             _ => ()
        //         }

        //     },
        //     Ok(Frame::Short(_)) => (),
        //     Err(err) => {
        //         println!("{}", err);

        //         let ts = start.elapsed().unwrap();
        //         writeln!(file, "{}: error: {}  buffer: {:x?}", ts.as_millis(), err, &buffer.make_contiguous()[..]).unwrap();
        //     },

    Ok(())
}
