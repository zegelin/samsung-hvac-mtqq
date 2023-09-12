use std::{sync::Arc, pin::Pin, collections::HashMap, time::SystemTime, alloc::System};

use anyhow::{Result, bail, Context};

use clap::{Parser};
use colored::{Colorize, ColoredString};
use futures::{Stream, StreamExt};
use samsunghvac2mqtt::{config::Port, protocol::{codec::{RxFrame, delta_ms, FrameId}, addresses::*, commands::*}};
use tokio::{net::{TcpListener, TcpStream}, sync::Mutex, io::{AsyncWriteExt, AsyncReadExt, AsyncWrite, AsyncRead, split, ReadHalf, WriteHalf}};
use tokio_serial::SerialPortBuilderExt;
use tokio_util::codec::Framed;
use url::Url;


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
    let args = Args::parse();

    let mut framed = Port::open(&args.port).await?.framed();

    fn addr_desc(id: FrameId, addr: u8) -> String {
        let desc = match addr {
            MAIN_WRC => "Main WRC".to_string(),
            SUB_WRC => "Sub WRC".to_string(),

            TRACKING_BROADCAST => "Tracking Broadcast".to_string(),
            ENUMERATION_BROADCAST => "Enum Broadcast".to_string(),
            BROADCAST => "Broadcast".to_string(),

            INDOOR_UNIT_FIRST..=INDOOR_UNIT_LAST => {
                let id = addr - INDOOR_UNIT_FIRST;
                format!("Indoor Unit {id}")
            },

            ERV_FIRST..=ERV_LAST => {
                let id = addr - ERV_FIRST;
                format!("ERV {id}")
            },

            _ => format!("Unknown")
        };

        format!("{addr:02x}: {desc: <18}")
    }

    fn cmd_desc(id: FrameId, data: Vec<u8>) -> String {
        let desc = match id.into() {
            (MAIN_WRC | SUB_WRC, _, Command52Request::ID) => "Info 1 Request".to_string(),
            (INDOOR_UNIT_FIRST..=INDOOR_UNIT_LAST, MAIN_WRC | SUB_WRC, Command52IndoorUnitResponse::ID) => "IU Info 1 Response".to_string(),

            (MAIN_WRC | SUB_WRC, _, Command52Request::ID) => "Info 2 Request".to_string(),
            (INDOOR_UNIT_FIRST..=INDOOR_UNIT_LAST, MAIN_WRC | SUB_WRC, Command53IndoorUnitResponse::ID) => "IU Info 2 Response".to_string(),

            (MAIN_WRC | SUB_WRC, _, Command54Request::ID) => "Info 3 Request".to_string(),
            (INDOOR_UNIT_FIRST..=INDOOR_UNIT_LAST, MAIN_WRC | SUB_WRC, Command54IndoorUnitResponse::ID) => "IU Info 3 Response".to_string(),

            (MAIN_WRC | SUB_WRC, _, Command55Request::ID) => "Info 4 Request".to_string(),
            (INDOOR_UNIT_FIRST..=INDOOR_UNIT_LAST, MAIN_WRC | SUB_WRC, Command55IndoorUnitResponse::ID) => "IU Info 4 Response".to_string(),

            (MAIN_WRC, SUB_WRC, CommandC4Request::ID) => "Sub Handover 1".to_string(),
            (MAIN_WRC, SUB_WRC, CommandC5Request::ID) => "Sub Handover 2".to_string(),

            (MAIN_WRC, BROADCAST, CommandD1::ID) => "End of Phase".to_string(),
            (SUB_WRC, MAIN_WRC, CommandD1::ID) => "Bus Reset".to_string(),

            _ => format!("Unknown")
        };
        let cmd = id.cmd;
        format!("{cmd:02x}: {desc: <20} {data:02x?}")
    }

    fn coloured(id: FrameId, line: String) -> ColoredString {
        match id.into() {
            (MAIN_WRC, BROADCAST, _) => line.on_cyan().bright_white(),
            (MAIN_WRC, SUB_WRC, _) => line.on_purple().bright_white(),
            (SUB_WRC, MAIN_WRC, 0xd1) => line.on_red().bright_white(),
            (SUB_WRC, MAIN_WRC, _) => line.on_bright_purple().bright_white(),
            (MAIN_WRC, INDOOR_UNIT_FIRST..=INDOOR_UNIT_LAST, _) => line.on_green().bright_white(),
            (INDOOR_UNIT_FIRST..=INDOOR_UNIT_LAST, MAIN_WRC, _) => line.on_bright_green().bright_white(),
            (MAIN_WRC, ERV_FIRST..=ERV_LAST, _) => line.on_magenta().bright_white(),
            (ERV_FIRST..=ERV_LAST, MAIN_WRC, _) => line.on_bright_magenta().bright_white(),
            _ => line.on_black()
        }
    }

    let start_time = SystemTime::now();
    let mut last_frame_time: Option<SystemTime> = None;

    while let Some(Ok(frame)) = framed.next().await {
        let start_delta_ms = delta_ms(Some(start_time));
        let last_frame_delta_ms = delta_ms(last_frame_time);

        let (id, data) = match frame {
            RxFrame::Long(frame) => (frame.id, frame.data.to_vec()),
            RxFrame::Short(frame) => (frame.id, frame.data.to_vec()),
            RxFrame::Corrupted(data) => {
                println!("corrupted frame: {data:02x?}");
                continue;
            },
        };


        let src = addr_desc(id, id.src);
        let dst = addr_desc(id, id.dst);
        let cmd = cmd_desc(id, data);

        let line = format!("[{start_delta_ms:8}, {last_frame_delta_ms:8}] {src} -> {dst}: {cmd}");

        println!("{}", coloured(id, line));

        last_frame_time = Some(SystemTime::now());
    }




    

    Ok(())

}