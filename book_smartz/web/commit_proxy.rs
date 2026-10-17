//! Application version of the storage COMMIT-acknowledgement transport fixture.
//! Multiplex real pooled connections and interrupt only after the target reader's
//! ranking is committed. No production hook or synthetic success is involved.
use sqlx::PgPool;
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Notify,
    task::{JoinHandle, JoinSet},
};

use book_smartz_domain::ReaderId;

pub struct CommitProxy {
    port: u16,
    reached: Arc<Notify>,
    release: Arc<Notify>,
    task: JoinHandle<()>,
}
impl CommitProxy {
    pub async fn new(direct: &PgPool, reader: ReaderId) -> anyhow::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let options = direct.connect_options();
        let upstream = (options.get_host().to_owned(), options.get_port());
        let direct = direct.clone();
        let reached = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let seen = reached.clone();
        let resume = release.clone();
        let claimed = Arc::new(AtomicBool::new(false));
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    accepted=listener.accept() => {
                        let Ok((mut client,_))=accepted else {break};
                        let upstream=upstream.clone();let pool=direct.clone();
                        let seen=seen.clone();let resume=resume.clone();let claimed=claimed.clone();
                        connections.spawn(async move {
                            let mut server=TcpStream::connect(upstream).await?;
                            let (mut client_read,mut client_write)=client.split();
                            let (mut server_read,mut server_write)=server.split();
                            let forward=async {tokio::io::copy(&mut client_read,&mut server_write).await.map(|_|())};
                            let backward=async {
                                loop {
                                    let tag=server_read.read_u8().await?;
                                    let length=server_read.read_u32().await?;
                                    if !(4..=16*1024*1024).contains(&length) {return Err(io::Error::new(io::ErrorKind::InvalidData,"invalid PostgreSQL frame"))}
                                    let mut body=vec![0;(length-4) as usize];server_read.read_exact(&mut body).await?;
                                    if tag==b'C' && body==b"COMMIT\0" && !claimed.load(Ordering::SeqCst) {
                                        let saved: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM book_smartz.ranking_events WHERE reader_id=$1)").bind(reader.as_uuid()).fetch_one(&pool).await.map_err(io::Error::other)?;
                                        if saved && claimed.compare_exchange(false,true,Ordering::SeqCst,Ordering::SeqCst).is_ok() {
                                            seen.notify_one();resume.notified().await;
                                            return Ok(()); // Withhold the real completion and close transport.
                                        }
                                    }
                                    client_write.write_u8(tag).await?;client_write.write_u32(length).await?;
                                    client_write.write_all(&body).await?;client_write.flush().await?;
                                }
                            };
                            tokio::select! {result=forward=>result,result=backward=>result}
                        });
                    }
                    Some(_)=connections.join_next(), if !connections.is_empty() => {}
                }
            }
        });
        Ok(Self {
            port,
            reached,
            release,
            task,
        })
    }
    pub fn database(&self, direct: &str) -> anyhow::Result<String> {
        let mut url = url::Url::parse(direct)?;
        url.set_host(Some("127.0.0.1"))?;
        url.set_port(Some(self.port))
            .map_err(|()| anyhow::anyhow!("invalid port"))?;
        url.query_pairs_mut().append_pair("sslmode", "disable");
        Ok(url.to_string())
    }
    pub async fn commit_reached(&self) -> anyhow::Result<()> {
        tokio::time::timeout(Duration::from_secs(20), self.reached.notified()).await?;
        Ok(())
    }
    pub fn disconnect(&self) {
        self.release.notify_one();
    }
}
impl Drop for CommitProxy {
    fn drop(&mut self) {
        self.task.abort();
    }
}
