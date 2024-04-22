use bevy_ecs::system::CommandQueue;
use smol::channel::{TryRecvError, TrySendError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProcessorError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Deferred recv Error occurred: {0}")]
    DeferredRecv(#[from] TryRecvError),
    #[error("Deferred send Error occurred: {0}")]
    DeferredSend(#[from] TrySendError<CommandQueue>),
    #[error(transparent)]
    DeserializeError(#[from] serde::de::value::Error)
}

pub type ProcessorResult<T> = Result<T, ProcessorError>;
