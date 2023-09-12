use std::time::Duration;

use futures::{StreamExt, TryStreamExt, SinkExt};
use samsunghvac2mqtt::{protocol::{commands::*, codec::*, addresses::*}, config::PortStream};
use tokio::{sync::{mpsc, broadcast}, time::sleep};


use anyhow::Result;


enum NextCommand {
    Info1,
    Info2,
    Info3,
    Info4,

}

struct IndoorUnitState {
    power: bool,
}


pub async fn sub_wrc_task(mut port: Box<dyn PortStream>) -> Result<()> {

    /// Force a bus reset
    /// Wait for the main WRC to enumerate the bus
    /// Observe the enumeration and make note of the 

    // port.try_filter_map(|frame| )

    while let Some(frame) = port.try_next().await? {
        let frame_id = if let Some(id) = frame.id() { id } else { continue };


        let resp: TxFrame = match frame_id.into() {
            (MAIN_WRC, SUB_WRC, CommandC4Request::ID) => {
                let ping = frame.unpack_as::<CommandC4Request>()?;

                LongFrame::pack(SUB_WRC, MAIN_WRC, CommandC4Reply::default())?.into()
            },

            (MAIN_WRC, SUB_WRC, CommandC5Request::ID) => {
                let ping = frame.unpack_as::<CommandC5Request>()?;

                LongFrame::pack(SUB_WRC, MAIN_WRC, CommandC5Response::default())?.into()
            }

            // (MAIN_WRC, BROADCAST, EndOfPhase::ID) => {
            //     LongFrame::new(SUB_WRC, MAIN_WRC, 0xd1, [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]).into()
            // }

            _ => continue
        };

        port.send(resp).await?;
    };

    Ok(())

}