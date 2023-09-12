
#[derive(Clone, Copy)]
pub enum IndoorUnitMode {
    Auto,
    Heat,
    Cool,
    Dry,
    Fan
}

#[derive(Clone, Copy)]
pub enum IndoorUnitFanSpeed {
    Auto,
    Low,
    Medium,
    High
}

pub struct IndoorUnitState {
    pub power: bool,

    pub mode: IndoorUnitMode,

    pub fan_speed: IndoorUnitFanSpeed,

    pub setpoint_temp: u8,

    pub defrost: bool,

    pub s_plasma_ion: bool,

    pub clean_filter: bool,

    pub humidifier: bool,

    pub oa_intake: bool,

    pub away_mds: bool,
}