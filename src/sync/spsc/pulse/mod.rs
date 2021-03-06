//! A single-producer, single-consumer queue for sending pulses across
//! asynchronous tasks.
//!
//! See [`channel`] constructor for more.

mod receiver;
mod sender;

pub use self::{
    receiver::Receiver,
    sender::{SendError, Sender},
};

use crate::sync::spsc::{SpscInner, SpscInnerErr};
use alloc::sync::Arc;
use core::{
    cell::UnsafeCell,
    mem::{size_of, MaybeUninit},
    sync::atomic::{AtomicUsize, Ordering},
    task::Waker,
};

/// Maximum capacity of the channel.
pub const MAX_CAPACITY: usize = 1 << size_of::<usize>() as u32 * 8 - OPTION_BITS;

#[allow(clippy::identity_op)]
const TX_WAKER_STORED: usize = 1 << 0;
const RX_WAKER_STORED: usize = 1 << 1;
const COMPLETE: usize = 1 << 2;
const OPTION_BITS: u32 = 3;

struct Inner<E> {
    state: AtomicUsize,
    err: UnsafeCell<Option<E>>,
    rx_waker: UnsafeCell<MaybeUninit<Waker>>,
    tx_waker: UnsafeCell<MaybeUninit<Waker>>,
}

/// Creates a new pulse channel, returning the sender/receiver halves.
///
/// The [`Sender`] half is used to signal a number of pulses. The [`Receiver`]
/// half is a [`Stream`](futures::stream::Stream) that reads the number of
/// pulses signaled from the last polling.
#[inline]
pub fn channel<E>() -> (Sender<E>, Receiver<E>) {
    let inner = Arc::new(Inner::new());
    let sender = Sender::new(Arc::clone(&inner));
    let receiver = Receiver::new(inner);
    (sender, receiver)
}

unsafe impl<E: Send> Send for Inner<E> {}
unsafe impl<E: Send> Sync for Inner<E> {}

impl<E> Inner<E> {
    #[inline]
    fn new() -> Self {
        Self {
            state: AtomicUsize::new(0),
            err: UnsafeCell::new(None),
            rx_waker: UnsafeCell::new(MaybeUninit::zeroed()),
            tx_waker: UnsafeCell::new(MaybeUninit::zeroed()),
        }
    }
}

impl<E> SpscInner<AtomicUsize, usize> for Inner<E> {
    const COMPLETE: usize = COMPLETE;
    const RX_WAKER_STORED: usize = RX_WAKER_STORED;
    const TX_WAKER_STORED: usize = TX_WAKER_STORED;
    const ZERO: usize = 0;

    #[inline]
    fn state_load(&self, order: Ordering) -> usize {
        self.state.load(order)
    }

    #[inline]
    fn compare_exchange_weak(
        &self,
        current: usize,
        new: usize,
        success: Ordering,
        failure: Ordering,
    ) -> Result<usize, usize> {
        self.state.compare_exchange_weak(current, new, success, failure)
    }

    #[inline]
    unsafe fn rx_waker_mut(&self) -> &mut MaybeUninit<Waker> {
        &mut *self.rx_waker.get()
    }

    #[inline]
    unsafe fn tx_waker_mut(&self) -> &mut MaybeUninit<Waker> {
        &mut *self.tx_waker.get()
    }
}

impl<E> SpscInnerErr<AtomicUsize, usize> for Inner<E> {
    type Error = E;

    unsafe fn err_mut(&self) -> &mut Option<Self::Error> {
        &mut *self.err.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::{
        num::NonZeroUsize,
        pin::Pin,
        sync::atomic::AtomicUsize,
        task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
    };
    use futures::stream::Stream;

    struct Counter(AtomicUsize);

    impl Counter {
        fn to_waker(&'static self) -> Waker {
            unsafe fn clone(counter: *const ()) -> RawWaker {
                RawWaker::new(counter, &VTABLE)
            }
            unsafe fn wake(counter: *const ()) {
                (*(counter as *const Counter)).0.fetch_add(1, Ordering::SeqCst);
            }
            static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake, drop);
            unsafe { Waker::from_raw(RawWaker::new(self as *const _ as *const (), &VTABLE)) }
        }
    }

    #[test]
    fn send_sync() {
        static COUNTER: Counter = Counter(AtomicUsize::new(0));
        let (mut tx, mut rx) = channel::<()>();
        assert_eq!(tx.send(1).unwrap(), ());
        drop(tx);
        let waker = COUNTER.to_waker();
        let mut cx = Context::from_waker(&waker);
        assert_eq!(
            Pin::new(&mut rx).poll_next(&mut cx),
            Poll::Ready(Some(Ok(NonZeroUsize::new(1).unwrap())))
        );
        assert_eq!(Pin::new(&mut rx).poll_next(&mut cx), Poll::Ready(None));
        assert_eq!(COUNTER.0.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn send_async() {
        static COUNTER: Counter = Counter(AtomicUsize::new(0));
        let (mut tx, mut rx) = channel::<()>();
        let waker = COUNTER.to_waker();
        let mut cx = Context::from_waker(&waker);
        assert_eq!(Pin::new(&mut rx).poll_next(&mut cx), Poll::Pending);
        assert_eq!(tx.send(1).unwrap(), ());
        assert_eq!(
            Pin::new(&mut rx).poll_next(&mut cx),
            Poll::Ready(Some(Ok(NonZeroUsize::new(1).unwrap())))
        );
        assert_eq!(Pin::new(&mut rx).poll_next(&mut cx), Poll::Pending);
        drop(tx);
        assert_eq!(Pin::new(&mut rx).poll_next(&mut cx), Poll::Ready(None));
        assert_eq!(COUNTER.0.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn send_err() {
        static COUNTER: Counter = Counter(AtomicUsize::new(0));
        let (tx, mut rx) = channel::<()>();
        assert_eq!(tx.send_err(()).unwrap(), ());
        let waker = COUNTER.to_waker();
        let mut cx = Context::from_waker(&waker);
        assert_eq!(Pin::new(&mut rx).poll_next(&mut cx), Poll::Ready(Some(Err(()))));
        assert_eq!(Pin::new(&mut rx).poll_next(&mut cx), Poll::Ready(None));
        assert_eq!(COUNTER.0.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn recv_many() {
        static COUNTER: Counter = Counter(AtomicUsize::new(0));
        let (mut tx, mut rx) = channel::<()>();
        let waker = COUNTER.to_waker();
        let mut cx = Context::from_waker(&waker);
        assert_eq!(Pin::new(&mut rx).poll_next(&mut cx), Poll::Pending);
        assert_eq!(tx.send(1).unwrap(), ());
        assert_eq!(tx.send(1).unwrap(), ());
        assert_eq!(tx.send(1).unwrap(), ());
        assert_eq!(
            Pin::new(&mut rx).poll_next(&mut cx),
            Poll::Ready(Some(Ok(NonZeroUsize::new(3).unwrap())))
        );
        assert_eq!(Pin::new(&mut rx).poll_next(&mut cx), Poll::Pending);
        drop(tx);
        assert_eq!(Pin::new(&mut rx).poll_next(&mut cx), Poll::Ready(None));
        assert_eq!(COUNTER.0.load(Ordering::SeqCst), 4);
    }
}
