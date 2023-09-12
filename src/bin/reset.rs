use std::{sync::Arc, pin::Pin, collections::HashMap, time::SystemTime, alloc::System, process};

use anyhow::{Result, bail, Context};

use clap::{Parser};
use colored::{Colorize, ColoredString};
use futures::{Stream, StreamExt, TryStreamExt, SinkExt};
use samsunghvac2mqtt::{config::Port, protocol::{codec::{RxFrame, delta_ms, FrameId, LongFrame}, addresses::*, commands::*}};
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

    println!("Waiting for end-of-phase broadcast...");

    while let Some(frame) = framed.try_next().await? {
        if let Some(id) = frame.id() {
            match id.into() {
                (MAIN_WRC, BROADCAST, CommandD1::ID) => {

                    let frame = LongFrame::pack(SUB_WRC, MAIN_WRC, CommandD1Response::default() )?.into();

                    framed.send(frame).await?;

                    println!("Sent reset! Main WRC should now be rebooting.");

                    process::exit(0);
                },

                _ => ()
            }
        }
    }

    Ok(())

}