use std::sync::atomic::{AtomicU32, Ordering};

use async_broadcast::broadcast;
use futures::{StreamExt, SinkExt};
use tokio::{io::{AsyncRead, AsyncWrite}, select};

use core::fmt::Debug;

type BusClientId = u32;

const EXTERNAL_CLIENT_ID: u32 = 0;


pub struct Bus<Rx, Tx> {
    rx_frames_send: async_broadcast::Sender<(BusClientId, Rx)>,
    tx_frames_send: async_broadcast::Sender<(BusClientId, Tx)>,

    next_client_id: AtomicU32
}

// type RxFrame = Clone + Debug;
// trait TxFrame<Rx>: Clone + Into<Rx> + Debug {}

// trait Codec<T>: Encoder<T> + Decoder {}

impl<Rx, Tx> Bus<Rx, Tx> where
    Rx: Clone + Sync + Send +'static,
    Tx: Into<Rx> + Clone + Send + Sync + 'static
{

    pub fn new<>() -> Self {
        let (rx_frames_send, _) = broadcast(1);
        let (tx_frames_send, _) = broadcast(1);

        // spawn a task to broadcast all sent TX frames to all RX receivers
        // (the BusClient filters out messages sent to self)
        tokio::spawn({
            let mut tx_frames_recv = tx_frames_send.new_receiver();
            let rx_frames_send = rx_frames_send.clone();

            async move {
                loop {
                    let (client_id, frame): (BusClientId, Tx) = tx_frames_recv.recv().await.expect("recv on tx_frames_recv");
                    rx_frames_send.broadcast((client_id, frame.into())).await;
                }
            }
        });

        Self {
            rx_frames_send,
            tx_frames_send,

            next_client_id: AtomicU32::new(EXTERNAL_CLIENT_ID + 1)
        }
    }

    pub fn new_client(&self) -> BusClient<Rx, Tx> {
        let id = self.next_client_id.fetch_add(1, Ordering::SeqCst);

        self._new_client(id)
    }

    fn _new_client(&self, id: BusClientId) -> BusClient<Rx, Tx> {
        BusClient {
            id,
            rx_frames: self.rx_frames_send.new_receiver(),
            tx_frames: self.tx_frames_send.clone()
        }
    }

    /// Attach a port (a `Stream` of `Rx` frames and `Sink` of `Tx` frames) to the bus.
    pub fn attach_port<T>(&self, port: T) where
        T: StreamExt<Item = anyhow::Result<Rx>> + SinkExt<Tx> + Unpin
    {
        let client = self._new_client(EXTERNAL_CLIENT_ID);

        // tokio::spawn(async {
        //     select! {
        //         rx_frame = port.next() => {
        //             match rx_frame {
        //                 Some(Ok(frame)) => {
        //                     //client.send(frame).expect("send on rx_frames_send");
        //                 },
        //                 Some(Err(err)) => {
        //                     // TODO: handle errors
        //                     println!("error in rx: {}", err);
        //                 }
        //                 None => {
        //                     println!("None in rx_frame?")
        //                 }
        //             }

        //         },
        //         tx_frame = client.recv() => {
        //             match tx_frame {
        //                 Some((client_id, frame)) => {
        //                     // write to port
        //                     //port.send(&frame).await.expect("send on port");
        //                 },
        //                 None => {
        //                     println!("None in tx_frame?")
        //                 }
        //             }
        //         }
        //     }
        // });
    }
}


pub struct BusClient<Rx, Tx> {
    id: BusClientId,

    rx_frames: async_broadcast::Receiver<(BusClientId, Rx)>,
    tx_frames: async_broadcast::Sender<(BusClientId, Tx)>
}

impl<Rx, Tx> BusClient<Rx, Tx> where
    Rx: Clone,
    Tx: Clone
{
    pub async fn recv(&mut self) -> Result<Rx, async_broadcast::RecvError> {
        loop {
            let (client_id, frame) = self.rx_frames.recv().await?;
            if client_id != self.id {
                return Ok(frame)
            }
        }
    }

    pub async fn send(&mut self, frame: Tx) {
        self.tx_frames.broadcast((self.id, frame)).await.expect("send on tx_frames");
    }
}