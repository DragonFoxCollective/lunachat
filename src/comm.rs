use async_trait::async_trait;
use bincode::Options as _;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};

use crate::error::Result;
use crate::state::BINCODE;
use crate::state::post::PostKey;

#[derive(Debug, Serialize, Deserialize)]
pub enum Request {
    Render(PostKey),
    Track(PostKey),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Response {
    Render(String),
}

#[async_trait]
pub trait StreamRW: Serialize + for<'de> Deserialize<'de> {
    async fn read<R: AsyncRead + Unpin + Send>(reader: &mut R) -> Result<Self> {
        let mut buf = [0; u8::MAX as usize];
        let n = reader.read_u8().await? as usize;
        let buf = &mut buf[..n];
        reader.read_exact(buf).await?;
        let data: Self = BINCODE.deserialize(buf)?;
        Ok(data)
    }

    async fn write<W: AsyncWrite + Unpin + Send>(&self, writer: &mut W) -> Result<()> {
        let data = BINCODE.serialize(&self)?;
        writer.write_u8(data.len() as u8).await?;
        writer.write_all(&data).await?;
        writer.flush().await?;
        Ok(())
    }
}

impl StreamRW for Request {}
impl StreamRW for Response {}
