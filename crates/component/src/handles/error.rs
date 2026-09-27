use domain::component::{Operation, Rejection};
use thiserror::Error;

pub(crate) type Result<T> = std::result::Result<T, OperationError>;

#[derive(Debug, Error)]
pub enum OperationError {
    #[error("cannot start {operation}: {rejection}")]
    Rejected {
        operation: Operation,
        rejection: Rejection,
    },

    #[error("cannot initialize: {0}")]
    NotInitializable(Rejection),

    #[error(transparent)]
    Component(#[from] domain::component::Error),

    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}
