use std::{time::Duration, thread};

use anyhow::{Result, bail};
use futures::{StreamExt, TryStream, TryStreamExt};
use samsunghvac2mqtt::protocol::codec::WrcBusProtocolCodec;
use tokio::{sync::{broadcast::{Sender, self, Receiver}, mpsc}, select, time::sleep, net::TcpStream};
use tokio_serial::{SerialPortBuilderExt};
use tokio_util::codec::Framed;


use std::fmt::Debug;



mod iuemu;
mod subwrc;




#[tokio::main]
async fn main() -> Result<()> {
    //let mut stream = TcpStream::connect("192.168.2.104:8899")?;

    // let mut port = tokio_serial::new("/dev/ttyUSB0", 2400)
    //     .stop_bits(tokio_serial::StopBits::One)
    //     .parity(tokio_serial::Parity::Even)
    //     .open_native_async()?;

    // let port = TcpStream::connect("192.168.2.104:8899").await?;

    let port = TcpStream::connect("localhost:3456").await?;




    let framed = Box::new(Framed::new(port, WrcBusProtocolCodec::new()));


    

    // framed.inspect_ok(|frame| {
    //     // handle tracking polarity
    //     // if frame.id() == Some((MAIN_WRC, TRACKING_BROADCAST, TrackingPolarityDetect::ID)) {

    //     // }
    // });



    let subwrc_task = tokio::spawn(subwrc::sub_wrc_task(framed));


    thread::park();

    Ok(())
}
