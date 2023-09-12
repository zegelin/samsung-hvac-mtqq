use futures::{AsyncRead, AsyncWrite, Stream, Sink, TryStream};
use tokio::{net::TcpStream};
use tokio_serial::{SerialStream, SerialPortBuilderExt};
use tokio_util::codec::Framed;
use url::Url;
use anyhow::{Result, Context, bail};

use crate::protocol::codec::{RxFrame, TxFrame, WrcBusProtocolCodec};


pub enum Port {
    Serial(SerialStream),
    TcpRaw(TcpStream)
}


pub trait PortStream: Stream<Item = std::io::Result<RxFrame>> + Sink<TxFrame, Error = std::io::Error> + Send + Unpin {

    // fn send_cmd<T: Command>(&self, src: u8, dst: u8, cmd: T) {
    //     let foo: TxFrame = todo!();

    //     self.send(foo)
    // }

}

impl<T> PortStream for T
where
    T: Stream<Item = std::io::Result<RxFrame>> + Sink<TxFrame, Error = std::io::Error> + Send + Unpin,
{}

//impl<T> Foo for Framed<T, WrcBusProtocolCodec> {}


impl Port {
    pub async fn open(url: &Url) -> Result<Self> {
        match url.scheme() {
            "serial" => {
                let path = url.path();

                let port = tokio_serial::new(path, 2400)
                    .stop_bits(tokio_serial::StopBits::One)
                    .parity(tokio_serial::Parity::Even)
                    .open_native_async()
                    .with_context(|| format!("failed to open serial port {path}"))
                    ?;

                Ok(Self::Serial(port))
            },
            "tcp+raw" => {
                let host = url.host_str()
                    .with_context(|| format!("tcp+raw requires a host to be specified in the url: {url}"))?;

                let port = url.port()
                    .with_context(|| format!("tcp+raw requires a port number to be specified in the url: {url}"))?;

                let stream = TcpStream::connect((host, port)).await
                    .with_context(|| format!("failed to open tcp+raw connection to: {url}"))?;

                stream.set_nodelay(true)?;

                Ok(Self::TcpRaw(stream))
            },
            other => {
                bail!("url scheme {other} not supported");
            }
        }
    }

    pub fn framed(self) -> Box<dyn PortStream> where
    {
        match self {
            Port::Serial(port) => {
                Box::new(Framed::new(port, WrcBusProtocolCodec::new()))

            },
            Port::TcpRaw(stream) => {
                Box::new(Framed::new(stream, WrcBusProtocolCodec::new()))
            }
        }
    }
}
