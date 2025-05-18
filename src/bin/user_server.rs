use lunachat::comm::{Request, Response, StreamRW};
use lunachat::error::{Error, Result};
use tokio::io::{BufReader, BufWriter};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

#[tokio::main]
async fn main() -> Result<()> {
    let listener = TcpListener::bind("0.0.0.0:8006").await?;

    loop {
        let (stream, _addr) = listener.accept().await?;
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream).await {
                eprintln!("Error: {}", e);
            }
        });
    }
}

async fn handle_connection(stream: TcpStream) -> Result<()> {
    let (reader, writer) = stream.into_split();
    let reader = BufReader::new(reader);
    let writer = BufWriter::new(writer);
    let (send, recv) = mpsc::unbounded_channel::<Response>();

    tokio::try_join!(read_requests(reader, send), write_responses(writer, recv)).map(|_| ())
}

async fn read_requests(
    mut reader: BufReader<OwnedReadHalf>,
    send: UnboundedSender<Response>,
) -> Result<()> {
    loop {
        let request = Request::read(&mut reader).await.map_err(|err| match err {
            Error::ConnectionClosed => Error::ConnectionClosed,
            Error::IO(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => {
                Error::ConnectionClosed
            }
            _ => err,
        })?;
        let response = Response::Render(format!("{:?}", request));
        send.send(response)?;
    }
}

async fn write_responses(
    mut writer: BufWriter<OwnedWriteHalf>,
    mut recv: UnboundedReceiver<Response>,
) -> Result<()> {
    loop {
        let response = recv.recv().await.ok_or(Error::ConnectionClosed)?;
        response.write(&mut writer).await?;
    }
}
