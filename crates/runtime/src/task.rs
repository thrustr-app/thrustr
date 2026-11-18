use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::task::{JoinError, JoinHandle};

/// A task that is aborted when its handle is dropped.
#[derive(Debug)]
pub struct AbortOnDrop<T>(pub JoinHandle<T>);

impl<T> Future for AbortOnDrop<T> {
    type Output = Result<T, JoinError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.0).poll(cx)
    }
}

impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}
