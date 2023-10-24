//! Docker abstraction to create, start and stop containers.
use shiplift::{errors::Error as ShipliftError, ContainerOptions, Docker, LogsOptions};
use shiplift::tty::TtyChunk;
use futures_util::stream::StreamExt;
use tracing::{debug, trace};

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

pub struct DockerManager {
    docker: Docker,
    image: String,
}

impl DockerManager {
    pub fn new(image: &str) -> Self {
        Self {
            docker: Docker::new(),
            image: image.to_string(),
        }
    }

    pub async fn create(&self, port: u32) -> Result<String, DockerError> {
        let c = self
            .docker
            .containers()
            .create(
                &ContainerOptions::builder(self.image.as_ref())
                    .expose(port, "tcp", port)
                    .cmd(vec!["katana", "--port", &port.to_string()])
                    .build(),
            )
            .await?;

        trace!("created {}", c.id);
        Ok(c.id)
    }

    pub async fn remove(&self, container_id: &str) -> Result<(), DockerError> {
        let c = self.docker.containers().get(container_id);
        trace!("stopping {}", container_id);
        c.stop(None).await?;
        trace!("deleting {}", container_id);
        c.delete().await?;
        Ok(())
    }

    pub async fn start(&self, container_id: &str) -> Result<(), DockerError> {
        trace!("starting {}", container_id);
        self.docker.containers().get(container_id).start().await?;
        Ok(())
    }

    pub async fn logs(&self, container_id: &str) -> Result<String, DockerError> {
        let mut output: String = String::new();

        let mut logs_stream = self.docker
            .containers()
            .get(container_id)
            .logs(&LogsOptions::builder().stdout(true).stderr(true).tail("all").build());

        while let Some(log_result) = logs_stream.next().await {
            match log_result {
                Ok(chunk) => {
                    match chunk {
                        TtyChunk::StdOut(bytes) => output.push_str(std::str::from_utf8(&bytes).unwrap()),
                        TtyChunk::StdErr(bytes) => output.push_str(std::str::from_utf8(&bytes).unwrap()),
                        TtyChunk::StdIn(_) => unreachable!(),
                    }

                }
                Err(e) => return Err(DockerError::Shiplift(e)),
            };
        };

        Ok(output)
    }
}
