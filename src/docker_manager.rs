//! Docker abstraction to create, start and stop containers.
use futures_util::stream::StreamExt;
use shiplift::tty::TtyChunk;
use shiplift::{
    errors::Error as ShipliftError, ContainerOptions, Docker, LogsOptions, RmContainerOptions,
};
use tracing::trace;

/// Errors for docker operations.
#[derive(Debug, thiserror::Error)]
pub enum DockerError {
    #[error("An error occurred: {0}")]
    Generic(String),
    #[error("Shiplift error: {0}")]
    Shiplift(ShipliftError),
}

impl From<ShipliftError> for DockerError {
    fn from(e: ShipliftError) -> Self {
        Self::Shiplift(e)
    }
}

#[derive(Clone)]
pub struct DockerManager {
    docker: Docker,
    image: String,
}

#[derive(Debug, Default)]
pub struct KatanaDockerOptions {
    pub port: u32,
    pub block_time: Option<u32>,
    pub no_mining: Option<bool>,
}

impl KatanaDockerOptions {
    pub fn to_str_vec(&self) -> Vec<String> {
        let mut out = vec![
            "katana".to_string(),
            "--port".to_string(),
            self.port.to_string(),
            "--disable-fee".to_string(),
        ];

        if let Some(v) = self.block_time {
            out.push("--block-time".to_string());
            out.push(v.to_string());
        }

        if let Some(v) = self.no_mining {
            out.push("--no-mining".to_string());
            out.push(v.to_string());
        }

        out
    }
}

impl DockerManager {
    pub fn new(image: &str) -> Self {
        Self {
            docker: Docker::new(),
            image: image.to_string(),
        }
    }

    pub async fn create(&self, opts: &KatanaDockerOptions) -> Result<String, DockerError> {
        let c = self
            .docker
            .containers()
            .create(
                &ContainerOptions::builder(self.image.as_ref())
                    .expose(opts.port, "tcp", opts.port)
                    .cmd(opts.to_str_vec().iter().map(|n| &**n).collect())
                    .build(),
            )
            .await?;

        trace!("created {} with opts {:?}", c.id, opts);
        Ok(c.id)
    }

    pub async fn remove(&self, container_id: &str, force: bool) -> Result<(), DockerError> {
        let c = self.docker.containers().get(container_id);

        if force {
            trace!("force removing {}", container_id);
            let opts = RmContainerOptions::builder().force(true).build();
            c.remove(opts).await?;
        } else {
            trace!("stopping {}", container_id);
            c.stop(None).await?;
            trace!("deleting {}", container_id);
            c.delete().await?;
        }

        Ok(())
    }

    pub async fn start(&self, container_id: &str) -> Result<(), DockerError> {
        trace!("starting {}", container_id);
        self.docker.containers().get(container_id).start().await?;
        Ok(())
    }

    pub async fn logs(&self, container_id: &str, n: String) -> Result<String, DockerError> {
        // TODO: n must be en enum All/Number.
        let mut output: String = String::new();

        let mut logs_stream = self.docker.containers().get(container_id).logs(
            &LogsOptions::builder()
                .stdout(true)
                .stderr(true)
                .tail(&n)
                .build(),
        );

        while let Some(log_result) = logs_stream.next().await {
            match log_result {
                Ok(chunk) => match chunk {
                    TtyChunk::StdOut(bytes) => {
                        output.push_str(std::str::from_utf8(&bytes).unwrap())
                    }
                    TtyChunk::StdErr(bytes) => {
                        output.push_str(std::str::from_utf8(&bytes).unwrap())
                    }
                    TtyChunk::StdIn(_) => unreachable!(),
                },
                Err(e) => return Err(DockerError::Shiplift(e)),
            };
        }

        Ok(output)
    }
}
