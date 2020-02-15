use futures::{
    stream::{FusedStream, Stream},
    task::Poll,
};
use std::{
    collections::VecDeque,
    error, fmt,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Waker},
};

pub struct QueueInner<T> {
    queue: VecDeque<T>,
    waker: Option<Waker>,
    receiver_dropped: bool,
}

#[derive(Clone)]
pub struct Sender<T>(Arc<Mutex<QueueInner<T>>>);

#[derive(Debug, Clone)]
pub enum SendError {
    ReceiverDropped,
}

impl error::Error for SendError {}

impl fmt::Display for SendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SendError::ReceiverDropped => write!(f, "ReceiverDropped")
        }
    }
}

#[derive(Debug, Clone)]
pub enum SinkError {
    ReceiverDropped,
    Closed,
}

impl error::Error for SinkError {}

impl fmt::Display for SinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SinkError::ReceiverDropped => write!(f, "ReceiverDropped"),
            SinkError::Closed => write!(f, "Closed"),
        }
    }
}

impl<T> Sender<T> {
    /// current queue len. This can be used to detect when the receiver is lagging
    pub fn queue_len(&self) -> usize {
        self.0.lock().unwrap().queue.len()
    }

    // true if the receiver is dropped, and therefore there is no point in sending anymore
    pub fn is_cancelled(&self) -> bool {
        self.0.lock().unwrap().receiver_dropped
    }

    pub fn send(&self, value: T) -> std::result::Result<usize, SendError> {
        let mut inner = self.0.lock().unwrap();
        if !inner.receiver_dropped {
            inner.queue.push_back(value);
            let len = inner.queue.len();
            // we only need to wake once
            for waker in inner.waker.take() {
                waker.wake_by_ref();
            }
            Ok(len)
        } else {
            Err(SendError::ReceiverDropped)
        }
    }

    pub fn sink(self) -> Sink<T> {
        Sink(Some(self))
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        // we might be the last sender, and the receiver might be waiting for us.
        // this will cause some false wakeups, but that's ok.
        for waker in self.0.lock().unwrap().waker.take() {
            waker.wake_by_ref();
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let mut inner = self.0.lock().unwrap();
        inner.receiver_dropped = true;
        inner.waker = None;
    }
}

pub struct Receiver<T>(Arc<Mutex<QueueInner<T>>>);

impl<T> Stream for Receiver<T> {
    type Item = T;
    fn poll_next(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Option<Self::Item>> {
        let mut inner = self.0.lock().unwrap();
        if let Some(value) = inner.queue.pop_front() {
            Poll::Ready(Some(value))
        } else if Arc::strong_count(&self.0) == 1 {
            Poll::Ready(None)
        } else {
            inner.waker = Some(ctx.waker().clone());
            Poll::Pending
        }
    }
}

impl<T> FusedStream for Receiver<T> {
    fn is_terminated(&self) -> bool {
        Arc::strong_count(&self.0) == 1
    }
}

pub struct Sink<T>(Option<Sender<T>>);

impl<T> futures::sink::Sink<T> for Sink<T> {
    type Error = SinkError;

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(if let Some(inner) = &self.0 {
            if !inner.is_cancelled() {
                Ok(())
            } else {
                Err(SinkError::ReceiverDropped)
            }
        } else {
            Err(SinkError::Closed)
        })
    }

    fn start_send(self: Pin<&mut Self>, item: T) -> Result<(), Self::Error> {
        if let Some(inner) = &self.0 {
            match inner.send(item) {
                Ok(_) => Ok(()),
                Err(SendError::ReceiverDropped) => Err(SinkError::ReceiverDropped),
            }
        } else {
            Err(SinkError::Closed)
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        self.0 = None;
        Poll::Ready(Ok(()))
    }
}

pub fn mpsc<T>() -> (Sender<T>, Receiver<T>) {
    let inner: Arc<Mutex<QueueInner<T>>> = Arc::new(Mutex::new(QueueInner {
        queue: VecDeque::new(),
        waker: None,
        receiver_dropped: false,
    }));
    (Sender(inner.clone()), Receiver(inner))
}

#[cfg(test)]
mod tests {

    #[test]
    fn smoke() {}
}
