use std::{sync::Arc, pin::Pin, collections::HashMap};

use anyhow::{Result, bail, Context};

use clap::{Parser};
use samsunghvac2mqtt::config::Port;
use tokio::{net::{TcpListener, TcpStream}, sync::Mutex, io::{AsyncWriteExt, AsyncReadExt, AsyncWrite, AsyncRead, split, ReadHalf, WriteHalf}};
use tokio_serial::SerialPortBuilderExt;
use url::Url;


/// A helper tool 
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Address and port to listen on (host:port) for bus clients
    listen: String,

    /// URLs of additional ports to attach to the bus.
    /// 
    /// either serial:///device/path or tcp+raw://host:port URLs supported 
    #[arg(long)]
    attach: Vec<Url>
}



struct SharedState {
    next_port_id: usize,
    writers: HashMap<usize, Pin<Box<dyn AsyncWrite + Send>>>
}

impl SharedState {
    fn new() -> Self {
        SharedState { next_port_id: 0, writers: Default::default() }
    }
}


#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let listener = TcpListener::bind(&args.listen).await.unwrap();

    let state = Arc::new(Mutex::new(SharedState::new()));

    for url in args.attach {
        let id = match Port::open(&url).await? {
            Port::Serial(port) => {
                attach_port(port, state.clone()).await
            },
            Port::TcpRaw(stream) => {
                attach_port(stream, state.clone()).await
            }
        };

        println!("{url} attached as client {id}");
    }

    println!("listening on {}", args.listen);

    loop {
        let (socket, addr) = listener.accept().await.unwrap();

        socket.set_nodelay(true)?;

        let id = attach_port(socket, state.clone()).await;

        println!("new connection from {addr} attached as client {id}");
    }
}

async fn attach_port<T>(socket: T, state: Arc<Mutex<SharedState>>) -> usize where
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static
{
    let (rx, tx) = split(socket);

    let id = {
        let mut state = state.lock().await;

        let id = state.next_port_id;
        state.next_port_id += 1;
        
        state.writers.insert(id, Box::pin(tx));

        id
    };

    tokio::spawn(async move {
        process(id, rx, state).await
    });

    id
}

async fn process<T>(id: usize, mut socket: ReadHalf<T>, state: Arc<Mutex<SharedState>>) where
    T: AsyncRead
{
    let mut buffer = vec![0; 256];

    loop {
        let n = socket.read(&mut buffer).await.expect("read");

        if n == 0 {
            break;
        }

        //println!("{id}: {:x?}", &buffer[0..n]);

        for (other_id, other_socket) in state.lock().await.writers.iter_mut() {
            if *other_id == id { continue }
            
            if let Err(err) = other_socket.write_all(&buffer[0..n]).await {
                println!("error while writing to socket for client {other_id}: {err}");
            }
        }
    }

    // socket was closed, no longer broadcast to it
    println!("client {id} disconnected");
    state.lock().await.writers.remove(&id);
}
