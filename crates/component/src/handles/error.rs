use crate::Operation;
use domain::component::Status;
use thiserror::Error;

pub(crate) type Result<T> = std::result::Result<T, OperationError>;

#[derive(Debug, Error)]
pub enum OperationError {
    #[error("Cannot start {operation} while the component is {status}")]
    NotAllowed {
        operation: Operation,
        status: Status,
    },

    #[error("Cannot start {operation} while another operation is running")]
    Busy { operation: Operation },

    #[error("Cannot record the result of the {operation}: the component changed state meanwhile")]
    StatusChanged { operation: Operation },

    #[error(transparent)]
    Component(#[from] domain::component::Error),

    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}
