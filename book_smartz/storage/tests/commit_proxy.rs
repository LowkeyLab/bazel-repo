//! Test-only PostgreSQL transport: withhold a real server COMMIT completion.
//! TLS is disabled only for this local fixture so backend frames remain visible.
use std::{io, sync::Arc, time::Duration};

use sqlx::{
    PgPool,
    postgres::{PgPoolOptions, PgSslMode},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Notify,
    task::JoinHandle,
};

pub struct CommitProxy {
    pub pool: PgPool,
    acknowledged: Arc<Notify>,
    release: Arc<Notify>,
    task: JoinHandle<io::Result<()>>,
}

impl CommitProxy {
    pub async fn new(direct: &PgPool) -> Self {
        Self::with_response(direct, None).await
    }

    pub async fn with_response(
        direct: &PgPool,
        response: Option<(&'static str, &'static str)>,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let options = direct.connect_options();
        let upstream = (options.get_host().to_owned(), options.get_port());
        let acknowledged = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let reached = acknowledged.clone();
        let resume = release.clone();
        let task = tokio::spawn(async move {
            let (mut client, _) = listener.accept().await?;
            let mut server = TcpStream::connect(upstream).await?;
            let (mut client_read, mut client_write) = client.split();
            let (mut server_read, mut server_write) = server.split();
            let forward = async {
                tokio::io::copy(&mut client_read, &mut server_write)
                    .await
                    .map(|_| ())
            };
            let backward = async {
                loop {
                    let tag = server_read.read_u8().await?;
                    let length = server_read.read_u32().await?;
                    if !(4..=16 * 1024 * 1024).contains(&length) {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "invalid PostgreSQL frame",
                        ));
                    }
                    let mut body = vec![0; (length - 4) as usize];
                    server_read.read_exact(&mut body).await?;
                    if tag == b'C' && body == b"COMMIT\0" {
                        reached.notify_one();
                        resume.notified().await;
                        if let Some((severity, code)) = response {
                            // Synthetic ErrorResponse exercises SQLx's real PostgreSQL decoder.
                            let error =
                                format!("S{severity}\0V{severity}\0C{code}\0Mfixture-secret\0\0");
                            client_write.write_u8(b'E').await?;
                            client_write
                                .write_u32(u32::try_from(error.len() + 4).unwrap())
                                .await?;
                            client_write.write_all(error.as_bytes()).await?;
                            client_write.flush().await?;
                        }
                        // Drop the transport without sending CommandComplete or ReadyForQuery.
                        return Ok(());
                    }
                    client_write.write_u8(tag).await?;
                    client_write.write_u32(length).await?;
                    client_write.write_all(&body).await?;
                    client_write.flush().await?;
                }
            };
            tokio::select! { result = forward => result, result = backward => result }
        });
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(10))
            .connect_with(
                (*options)
                    .clone()
                    .host("127.0.0.1")
                    .port(port)
                    .ssl_mode(PgSslMode::Disable),
            )
            .await
            .unwrap();
        Self {
            pool,
            acknowledged,
            release,
            task,
        }
    }

    pub async fn commit_reached(&self) {
        tokio::time::timeout(Duration::from_secs(20), self.acknowledged.notified())
            .await
            .expect("server did not acknowledge COMMIT");
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
