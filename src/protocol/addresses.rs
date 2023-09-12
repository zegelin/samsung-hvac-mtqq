
pub const INDOOR_UNIT_FIRST: u8 = 0x20;  // 0x20 - 0x3f
pub const INDOOR_UNIT_LAST: u8 = 0x3f;

pub const ERV_FIRST: u8 = 0x50; // 0x50 - 0x6f
pub const ERV_LAST: u8 = 0x6f;

pub const MAIN_WRC: u8 = 0x84;
pub const SUB_WRC: u8 = 0x85;

pub const BROADCAST: u8 = 0xad;

pub const ENUMERATION_BROADCAST: u8 = 0xc9;

/// Address used by the main WRC to broadcast tracking frames/
pub const TRACKING_BROADCAST: u8 = 0xeb;