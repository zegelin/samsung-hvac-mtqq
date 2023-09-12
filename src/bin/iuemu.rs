
use std::{time::Duration, fmt::Debug};

use anyhow::Result;
use clap::Parser;
use futures::{SinkExt, TryStreamExt};
use rand::Rng;
use samsunghvac2mqtt::{config::{PortStream, Port}, protocol::{iu::{IndoorUnitState, IndoorUnitMode, IndoorUnitFanSpeed}, addresses::*, commands::*, codec::{TxFrame, Unpack, ShortFrame, LongFrame, RxFrame}}};
use tokio::time::sleep;
use url::Url;

/// Emulator for Samsung Indoor Units
/// 
/// Multiple instances (up to 16) can be started to emulate multiple IUs
/// on the same bus.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// URL of the port to connect to
    /// 
    /// either serial:///device/path or tcp+raw://host:port URLs supported 
    port: Url,
}


#[tokio::main]
async fn main() -> Result<()> {
    //let args = Args::parse();
    let args = Args {
        port: Url::parse("tcp+raw://localhost:3456")?
    };

    let framed = Port::open(&args.port).await?.framed();

    indoor_unit_emulator(0, framed).await?;

    Ok(())
}


struct TrackingState {
    address: Address,
}

#[derive(Copy, Clone)]
enum Address {
    Temporary(u8),
    Assigned(u8)
}

impl Address {
    fn random_temporary() -> Self {
        Self::Temporary(rand::random())
    }
}

impl From<Address> for u8 {
    fn from(value: Address) -> Self {
        match value {
            Address::Temporary(v) => v,
            Address::Assigned(v) => v,
        }
    }
}



pub async fn indoor_unit_emulator(unit_id: u8, mut port: Box<dyn PortStream>) -> Result<()> {
    println!("Starting Indoor Unit {unit_id}");

    let mut state = IndoorUnitState {
        power: true,
        mode: IndoorUnitMode::Heat,
        fan_speed: IndoorUnitFanSpeed::Auto,
        setpoint_temp: 23,
        defrost: true,
        s_plasma_ion: false,
        clean_filter: true,
        humidifier: false,
        oa_intake: false,
        away_mds: false,
    };

    /// is the address in the rx frame for a indoor unit?
    fn is_iu_rxframe(iu_address: Address, dst: u8) -> bool {
        match iu_address {
            Address::Temporary(_) => false,
            Address::Assigned(address) => {
                let address = INDOOR_UNIT_FIRST + address + 10;
                dst == address
            }
        }
    }

    let mut address = Address::random_temporary();
    // really should be a temp address
    //

    loop {
        let frame = if let Some(frame) = port.try_next().await? { frame } else {
            // stream reached the end
            return Ok(())
        };

        let frame_id = if let Some(id) = frame.id() { id } else { continue };

        let resp: TxFrame = match frame_id.into() {
            // tracking broadcast frames
            (MAIN_WRC, TRACKING_BROADCAST, TrackingCommandFB::ID) => {
                let cmd = frame.unpack_as::<TrackingCommandFB>()?;

                match cmd.state {
                    TrackingDiscoverState::TrackingStart => {
                        address = Address::random_temporary();
                    },
                    TrackingDiscoverState::DiscoverUnassigned => {
                        // if this IU is already assigned an address, ignore
                        if let Address::Assigned(_) = address {
                            continue;
                        }

                        address = Address::random_temporary();
                    },
                    TrackingDiscoverState::DiscoverAssigned => {
                        // if this IU only has a temporary address, ignore
                        if let Address::Temporary(_) = address {
                            continue;
                        }
                    },
                };

                let delay = match address {
                    Address::Temporary(addr) => {
                        // let delay = rand::thread_rng().gen_range(100..8000);
                        // Duration::from_millis(delay)
                        Duration::from_millis(addr as u64 * 100)
                    },
                    Address::Assigned(addr) => {
                        Duration::from_millis(addr as u64 * 100)
                    },
                };

                sleep(delay).await;

                let resp = TrackingHello {
                    unknown: 0x00
                };
                
                ShortFrame::pack(address.into(), MAIN_WRC, resp)?.into()
            }
            (MAIN_WRC, dst, TrackingAssignAddress::ID) if dst == address.into() => {
                let frame = frame.unpack_as::<TrackingAssignAddress>()?;

                let resp = TrackingAddressAssigned {
                    address: frame.address
                };

                address = Address::Assigned(frame.address);

                let new_address = frame.address;
                println!("IU {unit_id} (on {dst:x}): address assigned {new_address:x}");

                ShortFrame::pack(dst, MAIN_WRC, resp)?.into()
            },

            // WRC commands to this IU
            (src @ (MAIN_WRC | SUB_WRC), dst, cmd) if is_iu_rxframe(address, dst) => {
                match cmd {
                    CommandA0::ID => {
                        let cmd = frame.unpack_as::<CommandA0>()?;

                        println!("{cmd:?}");
                        if let RxFrame::Long(frame) = frame {
                            for b in &frame.data[..] {
                                print!("{b:08b} ")
                            }
                            println!("")
                        }
        
                        state.setpoint_temp = cmd.setpoint_temp;

                        state.s_plasma_ion = cmd.s_plasma_ion;
                        state.clean_filter ^= cmd.reset_clean_filter;
                        state.humidifier = cmd.humidifier;
                        state.mode = cmd.mode.into();
                        state.power = cmd.power;

                        let mut frame = LongFrame::pack(dst, src, cmd)?;
                        frame.id.cmd = 0x50;

                        frame.into()
                    },
        
                    Command52Request::ID => {
                        frame.unpack_as::<Command52Request>()?;
        
                        let resp = Command52IndoorUnitResponse {
                            setpoint_temp: Temperature::new(state.setpoint_temp, TemperatureUnit::Celsius),
                            iu_room_temp: Temperature::new(20, TemperatureUnit::Celsius),
                            iu_eva_inlet_temp: Temperature::new(40, TemperatureUnit::Celsius),
                            fan_speed: 0,
                            power: state.power,
                            unknown_bit33: true,
                            unknown_bit34: true,
                            defrost: state.defrost,
                            mode: state.mode.into(),
                            clean_filter: state.clean_filter,
                            iu_eva_outlet_temp: Temperature::new(18, TemperatureUnit::Celsius),
                        };

        
                        let mut frame = LongFrame::pack(dst, src, resp)?;
                        frame.data[5] = 0b11000001;

                        frame.data[6] = 0b11111111;


                        
                        frame.into()
                    },
        
                    Command53Request::ID => {
                        frame.unpack_as::<Command53Request>()?;
        
                        let resp = Command53IndoorUnitResponse {
                            humidifier: state.humidifier,
                        };
        
                        LongFrame::pack(dst, src, resp)?.into()
                    }
        
                    Command54Request::ID => {
                        frame.unpack_as::<Command54Request>()?;
        
                        let resp = Command54IndoorUnitResponse {
                            oa_intake: state.oa_intake,
                            away_mds: state.away_mds,
                            unknown_bit58: false,
                            s_plasma_ion: state.s_plasma_ion,
                            unknown_bit60: false,
                            range_hood: false,
                            discharge_temp_control: false,
                        };
        
                        let mut frame = LongFrame::pack(dst, src, resp)?;

                        frame.data[0] = 0xff;

                        frame.into()
                    }
        
                    Command55Request::ID => {
                        frame.unpack_as::<Command55Request>()?;
        
                        let resp = Command55IndoorUnitResponse {
                            humidity: 1,
                            co2: 1,
                            unknown: [0x00; 7]
                        };
        
                        LongFrame::pack(dst, src, resp)?.into()
                    },

                    _ => continue
                }

            }

            _ => {
                continue;
            }
        };

        port.send(resp).await?;

            // },
            // State::Enumeration { mut done_52, mut done_54  } => {
            //     // TODO: this all assumes that "enumeration" is even a thing.
            //     // the main WRC doesn't switch to normal operation until its enumerated everything.
            //     // if the response packets aren't different then this state can be removed.

            //     // it might be more correct to combine this with the tracking state
                
            //     // currently once 0x52 and 0x54 are sent once to this IU its assumed this IU is enumerated
            //     // and the state switches to normal.

            //     // it might be more correct to count (0x84, 0xc9, [0xc6/0xc4]) packets as they get sent
            //     // and the end of each enumeration round.

            //     let frame = rx_frames.recv().await?;

            //     let iu_address: u8 = tracking_state.address.into(); // fixme: add base address

            //     if !is_wrc_iu_rxframe(iu_address, &frame) { continue }

            //     match frame {
            //         RxFrame::Long(frame) => {
            //             let resp = match frame.id.cmd {
            //                 IndoorUnitInfo1Request::ID => {
            //                     frame.unpack_as::<IndoorUnitInfo1Request>()?;

            //                     let resp = IndoorUnitInfo1Response {
            //                         setpoint_temp: Temperature::new(23, TemperatureUnit::Celsius),
            //                         iu_room_temp: Temperature::new(20, TemperatureUnit::Celsius),
            //                         iu_eva_inlet_temp: Temperature::new(40, TemperatureUnit::Celsius),
            //                         fan_speed: 0,
            //                         power: true,
            //                         defrost: true,
            //                         iu_eva_outlet_temp: Temperature::new(18, TemperatureUnit::Celsius),
            //                     };

            //                     done_52 = true;

            //                     LongFrame::pack(iu_address, frame.id.src, resp)?.into()
            //                 },

            //                 IndoorUnitInfo3Request::ID => {
            //                     frame.unpack_as::<IndoorUnitInfo3Request>()?;

            //                     let resp = IndoorUnitInfo3Response {
            //                         oa_intake: true,
            //                         away_mds: true,
            //                         unknown58: false,
            //                         s_plasma_ion: true,
            //                         unknown60: false,
            //                         range_hood: true,
            //                         discharge_temp_control: true,
            //                     };

            //                     done_54 = true;

            //                     LongFrame::pack(iu_address, frame.id.src, resp)?.into()
            //                 },

            //                 _ => {
            //                     println!("IU: Got unexpected frame during enumeration: {frame:?}");
            //                     continue;
            //                 }
            //             };

            //             if done_52 && done_54 {
            //                 state = State::NormalOperation;
            //             } else {
            //                 state = State::Enumeration { done_52, done_54 };
            //             }

            //             tx_frames.send(resp).await?;
            //         },
            //         other => {
            //             println!("IU: Got unexpected frame during enumeration: {other:?}");
            //             continue;
            //         }
            //     }
            // },
            // State::NormalOperation => {
            //     let frame = rx_frames.recv().await?;

            //     // todo: move into state
            //     let iu_address = tracking_state.address.into();

            //     if !is_wrc_iu_rxframe(iu_address, &frame) { continue }

            //     match frame {
            //         RxFrame::Long(frame) => {
            //             let resp = match frame.id.cmd {
            //                 C

            //                 _ => {
            //                     println!("IU: Got unexpected frame during normal operation: {frame:?}");
            //                     continue;
            //                 }
            //             };

            //             tx_frames.send(resp).await?;
            //         },
            //         other => {
            //             println!("IU: Got unexpected frame during normal operation: {other:?}");
            //             continue;
            //         }
            //     }
            // },
        // }
    }


}