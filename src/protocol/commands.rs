use std::fmt::Debug;

use packed_struct::prelude::*;

use super::iu::IndoorUnitMode;

// pub enum Address {
//     IndoorUnitBase = 0x20,  // 0x20 - 0x3f
//     ErvBase = 0x50, // 0x50 - 0x64

//     MainWrc = 0x84,
//     SubWrc = 0x85,

    
//     Broadcast = 0xc9,

//     TrackingBroadcast = 0xeb,

//     // 0xad  (seen packet 0xd1)
// }

// pub mod command {
//     const CHANGE_INDOOR_UNIT_SETTINGS_REQUEST: u8 = 0xA0;
// }

/*
    Packets:

    [0x60] = "EEPROM Write (page 0, 1, 2)",
    [0x61] = "EEPROM Write (page 3, 4, 5)",

    [0x70] = "EEPROM Read (page 0, 1, 2)",
    [0x71] = "EEPROM Read (page 3, 4, 5)"

    0x83 = ??
            210884: [32, 84, 20, 83, 0, ff, ff, ff, ff, ff, ff, ff, d8, 34]
            210958: [32, 20, 84, 83, 0, 6, 0, 0, 0, 0, 0, 0, 21, 34]
*/

pub trait Command {
    const ID: u8;

    fn check(&self) {
        // default checks nothing
    }
}


/// Define a long (8 byte) command that always contains `0x00`s
macro_rules! empty_long_command {
    ($(#[$meta:meta])* $name:ident, $id:expr) => {
        $(#[$meta])*
        #[derive(PackedStruct, Debug)]
        #[packed_struct(bit_numbering="msb0")]
        pub struct $name {
            _empty: [u8; 8]
        }

        impl Default for $name {
            fn default() -> Self {
                Self {
                    _empty: [0x00; 8]
                }
            }
        }

        impl Command for $name {
            const ID: u8 = $id;

            fn check(&self) {
                assert_eq!(self._empty, [0x00; 8])
            }
        }
    }
}

mod cmd_a0 {
    use super::*;

    #[derive(PrimitiveEnum, Clone, Copy, Debug)]
    pub enum Mode {
        Auto = 0,
        Cool = 1,
        Dry = 2,
        Fan = 3,
        Heat = 4
    }

    // TODO: some way to use a macro to generate these impls?
    impl From<IndoorUnitMode> for Mode {
        fn from(value: IndoorUnitMode) -> Self {
            match value {
                IndoorUnitMode::Auto => Self::Auto,
                IndoorUnitMode::Heat => Self::Heat,
                IndoorUnitMode::Cool => Self::Cool,
                IndoorUnitMode::Dry => Self::Dry,
                IndoorUnitMode::Fan => Self::Fan,
            }
        }
    }

    impl From<Mode> for IndoorUnitMode {
        fn from(value: Mode) -> Self {
            match value {
                Mode::Auto => Self::Auto,
                Mode::Cool => Self::Cool,
                Mode::Dry => Self::Dry,
                Mode::Fan => Self::Fan,
                Mode::Heat => Self::Heat,
            }
        }
    }

    enum FanSpeed {
        Auto = 0,
        Low = 2,
        Medium = 4,
        High = 5
    }


    /// Command `0xa0` -- Change Indoor Unit Settings (request).
    /// 
    /// Sent from a WRC to IUs on the bus to change their settings.
    /// IUs reply with command `0x50`.
    #[derive(PackedStruct, Debug)]
    #[packed_struct(bit_numbering="msb0")]
    pub struct CommandA0 {
        /// blade something something
        #[packed_field(bytes="0")]
        pub unknown_byte0: u8,

        #[packed_field(bytes="1")]
        pub unknown_byte1: u8,

        // byte 2

        #[packed_field(bits="16:18")]
        pub fan_speed: u8,

        #[packed_field(bits="19:23")]
        pub setpoint_temp: u8,

        // byte 3

        #[packed_field(bits="25")]
        pub s_plasma_ion: bool,

        #[packed_field(bits="26")]
        pub reset_clean_filter: bool,

        #[packed_field(bits="27")]
        pub humidifier: bool,

        #[packed_field(bits="29:31", ty="enum")]
        pub mode: Mode,

        #[packed_field(bits="32")]
        pub unknown_bit32: bool,

        #[packed_field(bits="33")]
        pub unknown_bit33: bool,

        /// Power state
        #[packed_field(bits="34")]
        pub power: bool,

        /// Unknown.
        /// Seems to mirror the value of `power`.
        /// But surely 2 bits to represent power isn't required?
        #[packed_field(bits="35")]
        pub unknown_bit35: bool,


        // #[packed_field(bytes="4")]
        // pub unknown_b4: u8,

        #[packed_field(bytes="5")]
        pub unknown_byte5: u8,

        #[packed_field(bytes="6")]
        pub unknown_byte6: u8,

        #[packed_field(bytes="7")]
        pub unknown_byte7: u8,
    }

    impl Command for CommandA0 {
        const ID: u8 = 0xa0;

        fn check(&self) {
            assert_eq!(self.power, self.unknown_bit35);
        }
    }
}

pub use cmd_a0::CommandA0;


mod cmd_a2 {
    use super::*;

    #[derive(PrimitiveEnum, Clone, Copy, Debug)]
    pub enum PowerState {
        On = 0b11,
        Off = 0b00
    }

    /// Command `0xa2` -- Change ERV Settings (request)
    /// 
    /// Sent from a WRC to ERVs on the bus to change their settings.
    /// TODO: Do ERVs reply with `0x51`
    #[derive(PackedStruct, Debug)]
    #[packed_struct(bit_numbering="msb0")]
    pub struct CommandA2 {
        #[packed_field(bits="16:18")]
        pub fan_speed: u8, // b000, b100, 101, 111

        #[packed_field(bits="25")]
        pub clean_up: bool,

        #[packed_field(bits="27:28")]
        pub other_mode: u8, // b00 = off, b01 = cool, b10 = heat, b11 = auto

        // 34-35: power
        #[packed_field(bits="34:35", ty="enum")]
        pub power: PowerState, // b00 = off, b11 = on (weird, why not just 1 bit?)
    }
}

pub use cmd_a2::CommandA2;



#[derive(PrimitiveEnum, Clone, Copy, Debug)]
pub enum TemperatureUnit {
    Celsius,
    Fahrenheit
}


#[derive(PackedStruct)]
#[packed_struct(bit_numbering="msb0")]
pub struct Temperature {
    #[packed_field(bits="0", ty="enum")]
    pub unit: TemperatureUnit,

    #[packed_field(bits="1:7")]
    pub raw_value: u8
}

impl Temperature {
    pub fn new(value: u8, unit: TemperatureUnit) -> Self {
        Self {
            unit: unit,
            raw_value: value + 55 // FIXME: only for celsius
        }
    }

    pub fn celsius(&self) -> u8 {
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

mod cmd_52 {
    use super::*;

    #[derive(PrimitiveEnum, Clone, Copy, Debug)]
    pub enum Mode {
        Auto = 0b0000,
        Heat = 0b0001,
        Cool = 0b0010,
        Dry =  0b0100,
        Fan =  0b1000
    }

    // TODO: some way to use a macro to generate these impls?
    impl From<IndoorUnitMode> for Mode {
        fn from(value: IndoorUnitMode) -> Self {
            match value {
                IndoorUnitMode::Auto => Self::Auto,
                IndoorUnitMode::Heat => Self::Heat,
                IndoorUnitMode::Cool => Self::Cool,
                IndoorUnitMode::Dry => Self::Dry,
                IndoorUnitMode::Fan => Self::Fan,
            }
        }
    }

    impl From<Mode> for IndoorUnitMode {
        fn from(value: Mode) -> Self {
            match value {
                Mode::Auto => Self::Auto,
                Mode::Cool => Self::Cool,
                Mode::Dry => Self::Dry,
                Mode::Fan => Self::Fan,
                Mode::Heat => Self::Heat,
            }
        }
    }

    pub enum FanSpeed {
        Auto,
        Low,
        Medium,
        High
    }



    empty_long_command!(
        /// Command `0x52` (request).
        /// 
        /// Sent from a WRC to an IU or ERV to read various settings.
        /// IUs reply with [Command52IndoorUnitResponse].
        /// ERVs reply with [Command52ErvResponse]
        Command52Request,
        0x52);


    /// Command `0x52` (IU response).
    /// 
    /// Sent from the IU to the requesting WRC in response to a [Command52Request].
    #[derive(PackedStruct, Debug)]
    #[packed_struct(bit_numbering="msb0")]
    pub struct Command52IndoorUnitResponse {
        // byte 0

        /// Setpoint Temperature
        #[packed_field(bytes="0")]
        pub setpoint_temp: Temperature,

        // byte 1

        /// Indoor Unit Room Temperature
        #[packed_field(bytes="1")]
        pub iu_room_temp: Temperature,

        // byte 2

        /// Indoor Unit Evaporator Inlet Temperature
        /// (WRC service menu: 3,2)
        #[packed_field(bytes="2")]
        pub iu_eva_inlet_temp: Temperature,

        // byte 3

        // Fan Speed 
        #[packed_field(bits="29:31")]
        pub fan_speed: u8,

        // byte 4

        #[packed_field(bits="32")]
        pub power: bool,

        #[packed_field(bits="33")]
        pub unknown_bit33: bool,

        #[packed_field(bits="34")]
        pub unknown_bit34: bool,

        #[packed_field(bits="35")]
        pub defrost: bool,

        #[packed_field(bits="36:39", ty="enum")]
        pub mode: Mode,


        #[packed_field(bits="43")]
        pub clean_filter: bool,

        // #[packed_field(bits="40")]
        // pub unknown_bit40: bool,

        // #[packed_field(bits="40")]
        // pub unknown_bit40: bool,

        // #[packed_field(bits="40")]
        // pub unknown_bit40: bool,

        // #[packed_field(bits="40")]
        // pub unknown_bit40: bool,

        //mode: 

        //#[packed_field(bytes="")]
        //unknown: [u8; 3],

        /// Indoor Unit Evaporator Outlet Temperature
        /// (WRC service menu: 3,3)
        #[packed_field(bytes="7")]
        pub iu_eva_outlet_temp: Temperature
    }

    impl Command for Command52IndoorUnitResponse {
        const ID: u8 = 0x52;
    }



    /// Command `0x52` (ERV response)
    /// 
    /// Sent from the ERV to the requesting WRC in response to a [Info1Request].
    #[derive(PackedStruct, Debug)]
    #[packed_struct(bit_numbering="msb0")]
    pub struct Command52ErvResponse {
        #[packed_field(bits="32")]
        pub power: bool,

        #[packed_field(bits="33")]
        pub clean_up: bool,
    }

    impl Command for Command52ErvResponse {
        const ID: u8 = 0x52;
    }
}

pub use cmd_52::{Command52Request, Command52IndoorUnitResponse, Command52ErvResponse};


mod cmd_53 {
    use super::*;

    empty_long_command!(Command53Request, 0x53);

    /// Command `0x53` (IU response)
    #[derive(PackedStruct, Debug)]
    #[packed_struct(bit_numbering="msb0")]
    pub struct Command53IndoorUnitResponse {
        #[packed_field(bits="59")]
        pub humidifier: bool
    }

    impl Command for Command53IndoorUnitResponse {
        const ID: u8 = 0x53;
    }
}

pub use cmd_53::{Command53Request, Command53IndoorUnitResponse};


mod cmd_54 {
    use super::*;

    empty_long_command!(Command54Request, 0x54);

    /// Command `0x54` (IU response)
    #[derive(PackedStruct, Debug)]
    #[packed_struct(bit_numbering="msb0")]
    pub struct Command54IndoorUnitResponse {
        /// Outdoor Air Intake
        #[packed_field(bits="56")]
        pub oa_intake: bool,
        
        /// Away/MDS (Motion Detect Sensor)
        #[packed_field(bits="57")]
        pub away_mds: bool,

        #[packed_field(bits="58")]
        pub unknown_bit58: bool,

        /// S-Plasma Ion
        #[packed_field(bits="59")]
        pub s_plasma_ion: bool,

        #[packed_field(bits="60")]
        pub unknown_bit60: bool,

        /// Range Hood (related to ERV)
        #[packed_field(bits="61")]
        pub range_hood: bool,

        /// Discharge temperature control
        #[packed_field(bits="62")]
        pub discharge_temp_control: bool,
    }

    impl Command for Command54IndoorUnitResponse {
        const ID: u8 = 0x54;
    }
}

pub use cmd_54::{Command54Request, Command54IndoorUnitResponse};



mod cmd_55 {
    use super::*;

    empty_long_command!(Command55Request, 0x55);

    /// Command `0x55` (IU response)
    #[derive(PackedStruct, Debug)]
    #[packed_struct(bit_numbering="msb0")]
    pub struct Command55IndoorUnitResponse {
        #[packed_field(bits="0..=3")]
        pub humidity: u8,

        #[packed_field(bits="4..=7")]
        pub co2: u8,

        pub unknown: [u8; 7]
    }

    impl Command for Command55IndoorUnitResponse {
        const ID: u8 = 0x55;
    }
}

pub use cmd_55::{Command55Request, Command55IndoorUnitResponse};


// mod cmd_63 {
//     use super::*;

//     empty_long_command!(Command63Request, 0x63);

//     /// Command `0x63`
//     #[derive(PackedStruct, Debug)]
//     #[packed_struct(bit_numbering="msb0")]
//     pub struct Command63Response {
//         unknown: [u8; 8]
//     }

//     impl Command for Command63Response {
//         const ID: u8 = 0x63;
//     }
// }

// pub use cmd_63::{Command63Request, Command63Response};


mod cmd_64 {
    use super::*;

    #[derive(PrimitiveEnum, Clone, Copy, Debug)]
    pub enum TemperatureProbeSource {
        IndoorUnitSensor,
        WiredRemoteSensor
    }

    #[derive(PackedStruct)]
    pub struct PrecisionTemperature {
        pub high: u8,
        pub low: u8
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

    /// Command `0x64`
    #[derive(PackedStruct, Debug)]
    #[packed_struct(bit_numbering="msb0")]
    pub struct Command64Request {
        #[packed_field(bytes="0")]
        pub unknown_b0: u8,

        // byte 1

        #[packed_field(bits="8..=14")]
        pub unknown_b1_8_14: u8,

        #[packed_field(bits="15", ty="enum")]
        pub active_temp_sensor: TemperatureProbeSource,



        #[packed_field(bytes="2..=3")]
        pub wrc_temperature: PrecisionTemperature,

        #[packed_field(bytes="4..=5")]
        pub iu_temperature: PrecisionTemperature,



        pub unknown2: u8,
        pub unknown3: u8
    }

    impl Command for Command64Request {
        const ID: u8 = 0x64;
    }
}

pub use cmd_64::{Command64Request};







/*
RX      370: [32, 84, 20, 70, 1, 0, 0, 0, 0, 0, 0, 0, d5, 34]
RX       79: [32, 20, 84, 70, b1, 60, cc, 45, e4, 77, 88, 7c, eb, 34]
seems to be reading the IU option code (or part of it anyway)

RX        0: [32, 84, 20, 71, 1, 0, 0, 0, 0, 0, 0, 0, d4, 34]
RX       94: [32, 20, 84, 71, 0, 56, 0, 0, 0, 0, 0, 0, 83, 34]

    01606C-1C544E-27788c-370065
     b160C  C45E4  77887  C0056

 */
struct EepromLowReadRequest {
    pub address: u8,

    /// Probably empty
    pub unknown: [u8; 7]
}

impl Command for EepromLowReadRequest {
    const ID: u8 = 0x70;
}

struct EepromLowReadResponse {
    pub data: [u8; 8]
}

impl Command for EepromLowReadResponse {
    const ID: u8 = 0x70;
}

struct EepromHighReadRequest {
    pub address: u8,

    /// Probably empty
    pub unknown: [u8; 7]
}

impl Command for EepromHighReadRequest {
    const ID: u8 = 0x71;
}

struct EepromHighReadResponse {
    pub data: [u8; 8]
}

impl Command for EepromHighReadResponse {
    const ID: u8 = 0x71;
}



/// Command `0xc4` -- Main to Sub WRC handover 1 (request)
/// 
/// Enumeration
/// General Operation
/// 
/// Sent from main WRC to sub WRC to:
///     - detect if the sub WRC is present; and
///     - tell the sub WRC to take a turn communicating with the IUs/ERVs.
/// 
/// The sub WRC replies to the main WRC with an empty '0xc4' frame.
/// The it runs one phase of communication to each IU/ERV.
/// 
/// If the sub doesn't reply during enumeration the main won't ping the sub during GO. 
/// 
/// Also sent from main WRC as a `0xc9` broadcast during enumeration.
/// The purpose of this is unknown.
#[derive(PackedStruct, Debug)]
#[packed_struct(bit_numbering="msb0")]
pub struct CommandC4Request {
    /// Unknown.
    /// 
    /// Always seems to be `0x00` during enumeration and `0x01` during normal operation.
    /// Maybe tells the sub WRC to "enable itself"?
    #[packed_field(bytes="0")]
    pub unknown_byte0: u8,

    /// Number of indoor units found during bus enumeration.
    #[packed_field(bytes="1")]
    pub number_of_indoor_units: u8,

    /// The address of some IU -- unknown.
    /// 
    /// Seems to be the "highest" IU address detected?
    /// e.g., when emulating a IU on the bus that has a hard coded address of `0x26`
    /// (with the only other IU on `0x20`) this field is `0x26`.
    #[packed_field(bytes="4")]
    pub unknown_iu_address: u8,

    /// Unknown.
    /// 
    /// Seems to be always `0x44` when sent from the main WRC.
    /// FIXME: not always the case -- `0x45` has been seen too (when an ERV is detected).
    #[packed_field(bytes="7")]
    pub unknown_byte7: u8,
}

impl Command for CommandC4Request {
    const ID: u8 = 0xc4;

    fn check(&self) {
        //assert_eq!(self.unknown_byte7, 0x44)
    }
}

empty_long_command!(
    CommandC4Reply,
    0xc4
);

/// Command `0xc5` -- Main to Sub WRC handover 2
#[derive(PackedStruct, Debug)]
#[packed_struct(bit_numbering="msb0")]
pub struct CommandC5Request {
    /// Unknown.
    /// 
    /// Seems to be always `0x22`.
    #[packed_field(bytes="0")]
    pub unknown_byte0: u8,

    /// Unknown.
    /// On main to sub, switches from `0x0` to `0x80` once enumeration is completed
    #[packed_field(bytes="1")]
    pub unknown_byte1: u8,

    pub unknown: [u8; 6]
}

impl Command for CommandC5Request {
    const ID: u8 = 0xc5;

    fn check(&self) {
        assert_eq!(self.unknown_byte0, 0x22)
    }
}

/// Command `0xc5` -- Sub to Main WRC Handover 2 (ack)
#[derive(PackedStruct, Debug)]
#[packed_struct(bit_numbering="msb0")]
pub struct CommandC5Response {
    /// Unknown.
    /// 
    /// Seems to be always `0x22`.
    #[packed_field(bytes="0")]
    pub unknown_byte0: u8,

    /// Unknown.
    /// 
    /// Seems to be always `[0x00, 7]`.
    pub unknown: [u8; 7]
}

impl Command for CommandC5Response {
    const ID: u8 = 0xc5;
}

impl Default for CommandC5Response {
    fn default() -> Self {
        CommandC5Response {
            unknown_byte0: 0x22,
            unknown: [0; 7]
        }
    }
}



/// Command `0xc6` -- Bus Status broadcast
/// 
/// Seems similar if not identical to `0xc4` except broadcast only.
#[derive(PackedStruct, Debug)]
#[packed_struct(bit_numbering="msb0")]
pub struct CommandC6 {
    /// Unknown.
    /// Seems to be always `0x22`
    #[packed_field(bytes="0")]
    pub unknown_byte0: u8,

    #[packed_field(bytes="1")]
    pub number_of_indoor_units: u8,

    pub unknown: [u8; 6],
}

impl Command for CommandC6 {
    const ID: u8 = 0xc6;

    fn check(&self) {
        assert_eq!(self.unknown_byte0, 0x22)
    }
}


empty_long_command!(
    /// Command `0xd1` -- End of Phase
    /// 
    /// Sent at the end of every command "phase"/"round" by the main WRC
    /// as a broadcast to `0xad`.
    /// 
    /// If the sub WRC replies to the main WRC when it sees this
    /// broadcast message the main WRC will reset.
    /// This allows a sub WRC to attach to the bus at any time and
    /// force tracking/enumeration so it can discover what IU/ERVs are present
    /// on the bus.
    CommandD1,
    0xd1
);

empty_long_command!(
    CommandD1Response,
    0xd1
);



/// Tracking command `0xfa` -- Polarity Detect
/// 
/// Sent in inverse by the main WRC so that other devices on the
/// bus know to invert their rx/tx pins. Unfortunatly that is custom circutry
/// (see the WRC schematic). Best we can do is detect this, error out,
/// and tell the user to swap the rx/tx pins.
#[derive(PackedStruct, Debug)]
#[packed_struct(bit_numbering="msb0")]
pub struct TrackingCommandFA {
    /// Unknown.
    /// Seems to be always `0xaa`.
    #[packed_field(bytes="0")]
    pub unknown_b0: u8,

    /// Unknown.
    /// Seems to be always `[0x00; 7]`.
    pub unknown: [u8; 7]
}

impl Default for TrackingCommandFA {
    fn default() -> Self {
        Self {
            unknown_b0: 0xaa,
            unknown: Default::default()
        }
    }
}

impl Command for TrackingCommandFA {
    const ID: u8 = 0xfa;

    fn check(&self) {
        assert_eq!(self.unknown_b0, 0xaa);
        assert_eq!(self.unknown, [0x00; 7])
    }
}


#[derive(PrimitiveEnum_u8, Debug, Copy, Clone)]
pub enum TrackingDiscoverState {
    /// start tracking & randomize addresses
    /// 
    /// - switch all units into tracking mode. (TODO: does the sub WRC detect this too?)
    /// - all units will randomize their addresses and reply from that address with `0xfc`.
    /// - the main WRC doesn't reply.
    TrackingStart = 2,

    /// units without an assigned address will re-randomize and reply with `0xfc`.
    /// those with an assigned address wont reply.
    /// the main WRC sends `0xfd` packets to the replies to assign addresses.
    DiscoverUnassigned = 1,

    /// units with an assigned address reply `0xfc`.
    /// the main WRC sends `0xfd` packets, to re-assign the same address.
    /// units without an assigned address don't reply.
    DiscoverAssigned = 0 // 
}

/// Tracking command `0xfb` -- Discover
/// 
/// Broadcast command sent by the main WRC to discover IUs and ERVs on the bus.
/// IUs and ERVs reply with `TrackingHello`.
#[derive(PackedStruct, Debug)]
#[packed_struct(bit_numbering="msb0")]
pub struct TrackingCommandFB {
    #[packed_field(bits="6:7", ty="enum")]
    pub state: TrackingDiscoverState,

    pub unknown: [u8; 7]
}

impl Command for TrackingCommandFB {
    const ID: u8 = 0xfb;
}


/// Tracking command `0xfc`.
/// 
/// Sent by IUs and ERVs in response to a `TrackingDiscover` broadcast command.
#[derive(PackedStruct, Debug)]
#[packed_struct(bit_numbering="msb0")]
pub struct TrackingHello {
    pub unknown: u8,
}

impl Command for TrackingHello {
    const ID: u8 = 0xfc;
}


/// Tracking command `0xfd`
/// 
/// Sent by the main WRC to assign an address (or "unit number") to the
/// device at a temporary address.
#[derive(PackedStruct, Debug)]
#[packed_struct(bit_numbering="msb0")]
pub struct TrackingAssignAddress {
    #[packed_field(bytes="0")]
    pub address: u8
}

impl Command for TrackingAssignAddress {
    const ID: u8 = 0xfd;
}


#[derive(PackedStruct, Debug)]
#[packed_struct(bit_numbering="msb0")]
pub struct TrackingAddressAssigned {
    pub address: u8
}

impl Command for TrackingAddressAssigned {
    const ID: u8 = 0xfe;
}





// (0x84, 0xeb, 0xfa, [0xaa, 0x00..]) == tracking, polarity incorrect packet