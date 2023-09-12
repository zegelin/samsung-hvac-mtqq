use std::time::Duration;

use clap::Parser;
use futures::SinkExt;
use samsunghvac2mqtt::{config::{PortStream, Port}, protocol::{codec::{LongFrame, ShortFrame, TxFrame}, commands::*, addresses::*}};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;
use tokio::{sync::{mpsc, broadcast}, time::sleep, select};
use anyhow::Result;
use url::Url;


/// Emulator for a Samsung Main WRC
/// 
/// Based on the MWR-WE10
/// 
/// Only a single main WRC (physical or emulated) can be on the bus at same time.
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

    main_wrc_emulator(framed).await?;

    Ok(())
}


enum State {
    Tracking,
    Enumeration,
    NormalOperation
}

#[derive(EnumIter)]
enum Commands {
    Info1,
    Info2,
    Info3,
    Info4,
}

pub async fn main_wrc_emulator(mut port: Box<dyn PortStream>) -> Result<()> {

    let iu_addresses: Vec<u8> = vec![0x20];//, 0x21, 0x22];

    let mut state = State::Tracking;

    // fn send_msg<T: Command>(mut port: Box<dyn PortStream>, src: u8, dst: u8, cmd: T) {
    //     let frame: TxFrame = cmd.into();
    // }

    // let req = ShortFrame::new(MAIN_WRC, TRACKING_BROADCAST, 0xf9, [0x00]).into();
    // port.send(req).await?;

    // sleep(Duration::from_millis(600)).await;

    // state = State::Enumeration;

    'outer:
    loop {
        match state {
            State::Tracking => {
                // let req = TrackingPolarityDetect::default();
                // let req = LongFrame::pack(MAIN_WRC, TRACKING_BROADCAST, req)?.into();
                // tx_frames.send(req).await?;

                // sleep(Duration::from_millis(180)).await;


                let req = TrackingCommandFB {
                    state: TrackingDiscoverState::DiscoverAssigned,
                    unknown: Default::default()
                };
                let req = LongFrame::pack(MAIN_WRC, TRACKING_BROADCAST, req)?.into();
                port.send(req).await?;

                sleep(Duration::from_millis(8000)).await;

                for i in 0..3 {
                    let req = TrackingAssignAddress {
                        address: i,
                    };
                    let req = ShortFrame::pack(MAIN_WRC, i, req)?.into();
                    port.send(req).await?;
    
                    sleep(Duration::from_millis(180)).await;
                }


                let req = ShortFrame::new(MAIN_WRC, TRACKING_BROADCAST, 0xf9, [0x00]).into();
                port.send(req).await?;

                sleep(Duration::from_millis(600)).await;

                state = State::Enumeration;
                
            },
            State::Enumeration => {
                let cmds = vec![false, true];

                for cmd in cmds {
                    // IUs
                    for addr in INDOOR_UNIT_FIRST..=INDOOR_UNIT_LAST {
                        let req = match cmd {
                            false => {
                                let req = Command52Request::new();
                                LongFrame::pack(MAIN_WRC, addr, req)?
                            },
                            true => {
                                let req = Command54Request::new();
                                LongFrame::pack(MAIN_WRC, addr, req)?
                            },
                        };

                        port.send(req.into()).await?;

                        sleep(Duration::from_millis(180)).await;
                    }

                    // ERVs
                    for addr in ERV_FIRST..=ERV_LAST {
                        let req = match cmd {
                            false => {
                                let req = Command52Request::new();
                                LongFrame::pack(MAIN_WRC, addr, req)?
                            },
                            true => {
                                let req = Command54Request::new();
                                LongFrame::pack(MAIN_WRC, addr, req)?
                            },
                        };

                        port.send(req.into()).await?;

                        sleep(Duration::from_millis(180)).await;
                    }

                    {
                        {
                            // sub wrc
                            let ping = CommandC5Request {
                                unknown_byte0: 0x22,
                                unknown_byte1: 0x00,
                                unknown: [0x00; 6]
                            };
                            let ping = LongFrame::pack(MAIN_WRC, SUB_WRC, ping)?.into();
                            port.send(ping).await?;

                            sleep(Duration::from_millis(180)).await;

                            let ping = CommandC4Request {
                                unknown_byte0: 0x00,
                                number_of_indoor_units: 1,
                                unknown: [0x00; 5],
                                unknown_byte7: 0x44
                            };
                            let ping = LongFrame::pack(MAIN_WRC, SUB_WRC, ping)?.into();
                            port.send(ping).await?;

                            sleep(Duration::from_millis(180)).await;
                        }

                        {
                            // bus broadcast
                            let ping = CommandC6 {
                                unknown_byte0: 0x22,
                                number_of_indoor_units: 1,
                                unknown: [0x20, 0xb0, 0, 0, 0, 0]
                            };
                            let ping = LongFrame::pack(MAIN_WRC, ENUMERATION_BROADCAST, ping)?.into();
                            port.send(ping).await?;

                            sleep(Duration::from_millis(180)).await;

                            let ping = CommandC4Request {
                                unknown_b0: 0x00,
                                number_of_indoor_units: 1,
                                unknown: [0x00; 5],
                                unknown_b7: 0x44
                            };
                            let ping = LongFrame::pack(MAIN_WRC, ENUMERATION_BROADCAST, ping)?.into();
                            port.send(ping).await?;

                            sleep(Duration::from_millis(180)).await;
                        }

                    }

                }

                state = State::NormalOperation;

            },
            State::NormalOperation => {
                loop {
                    for cmd in Commands::iter() {
                        for iu_address in &iu_addresses {
                            let req = match cmd {
                                Commands::Info1 => {
                                    let req = Command52Request::new();
                                    LongFrame::pack(MAIN_WRC, *iu_address, req)?
                                },
                                Commands::Info2 => {
                                    let req = Command52Request::new();
                                    LongFrame::pack(MAIN_WRC, *iu_address, req)?
                                },
                                Commands::Info3 => {
                                    let req = Command54Request::new();
                                    LongFrame::pack(MAIN_WRC, *iu_address, req)?
                                },
                                Commands::Info4 => {
                                    let req = Command55Request::new();
                                    LongFrame::pack(MAIN_WRC, *iu_address, req)?
                                },
                            };

                            let req = req.into();
                            port.send(req).await?;
                            
                            //let resp = rx_frames.recv().await?;
                            // TODO: validate resp;
                            // TODO: timeout

                            sleep(Duration::from_millis(180)).await;
                        }

                        // ping sub wrc
                        {
                            let ping = CommandC4Request {
                                unknown_b0: 0x01,
                                number_of_indoor_units: 1,
                                unknown: [0x10, 0xb6, 0x20, 0, 0],
                                unknown_b7: 0x44
                            };
                            let ping = LongFrame::pack(MAIN_WRC, SUB_WRC, ping)?.into();
                            port.send(ping).await?;

                            sleep(Duration::from_millis(900)).await;

                            let ping = CommandC5Request {
                                unknown_byte0: 0x22,
                                unknown_byte1: 0x80,
                                unknown: [0x00; 6]
                            };
                            let ping = LongFrame::pack(MAIN_WRC, SUB_WRC, ping)?.into();
                            port.send(ping).await?;

                            sleep(Duration::from_millis(900)).await;
                        }
                        
                        // end of phase broadcast
                        let bc = LongFrame::new(MAIN_WRC, 0xad, 0xd1, [0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]).into();
                        port.send(bc).await?;

                        // TODO: wait her for a response from the sub WRC -- if we get one, reset

                        sleep(Duration::from_millis(300)).await;

                        // select! {
                        //     resp = rx_frames.recv() => {
                        //         println!("{resp:?}");
                        //         if let Some(id) = resp?.id() {
                        //             let (src, dst, cmd) = id.into();
                        //             if src == SUB_WRC && dst == MAIN_WRC && cmd == 0xd1 {
                        //                 println!("Master WRC reset!");
                        //                 state = State::Discovery;
                        //                 continue 'outer;
                        //             }
                        //         }
                        //     },
                        //     _ = sleep(Duration::from_millis(360)) => {}
                        // }
                    }

                }



            },
        }

    }

    

    // for each command, for each iu
    // then 0xD1 broadcast to 0xad,



}
