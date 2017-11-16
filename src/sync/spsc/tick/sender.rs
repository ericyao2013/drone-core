use super::{Inner, COMPLETE, LOCK_BITS, LOCK_MASK, RX_LOCK};
use alloc::arc::Arc;
use core::sync::atomic::Ordering::*;
use futures::Poll;
use sync::spsc::SpscInner;

/// The sending-half of [`tick::channel`].
///
/// [`tick::channel`]: fn.channel.html
pub struct Sender<E> {
  inner: Arc<Inner<E>>,
}

/// Error returned from [`Sender::send_tick`].
///
/// [`Sender::send_tick`]: struct.Sender.html#method.send_tick
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendTickError {
  /// The corresponding [`Receiver`] is dropped.
  ///
  /// [`Receiver`]: struct.Receiver.html
  Canceled,
  /// The internal counter overflow. This may happen when the corresponding
  /// [`Receiver`] is too slow to consume the data.
  ///
  /// [`Receiver`]: struct.Receiver.html
  Overflow,
}

impl<E> Sender<E> {
  #[inline(always)]
  pub(super) fn new(inner: Arc<Inner<E>>) -> Self {
    Self { inner }
  }

  /// Sends a single tick across the channel.
  ///
  /// [`Receiver`]: struct.Receiver.html
  #[inline]
  pub fn send_tick(&mut self) -> Result<(), SendTickError> {
    self.inner.send_tick()
  }

  /// Completes this stream with an error.
  ///
  /// If the value is successfully enqueued, then `Ok(())` is returned. If the
  /// receiving end was dropped before this function was called, then `Err` is
  /// returned with the value provided.
  ///
  /// [`Receiver`]: struct.Receiver.html
  #[inline]
  pub fn send_err(self, err: E) -> Result<(), E> {
    self.inner.send_err(err)
  }

  /// Polls this [`Sender`] half to detect whether the [`Receiver`] this has
  /// paired with has gone away.
  ///
  /// # Panics
  ///
  /// Like `Future::poll`, this function will panic if it's not called from
  /// within the context of a task. In other words, this should only ever be
  /// called from inside another future.
  ///
  /// If you're calling this function from a context that does not have a task,
  /// then you can use the [`is_canceled`] API instead.
  ///
  /// [`Sender`]: struct.Sender.html
  /// [`Receiver`]: struct.Receiver.html
  /// [`is_canceled`]: struct.Receiver.html#method.is_canceled
  #[inline]
  pub fn poll_cancel(&mut self) -> Poll<(), ()> {
    self.inner.poll_cancel()
  }

  /// Tests to see whether this [`Sender`]'s corresponding [`Receiver`] has gone
  /// away.
  ///
  /// [`Sender`]: struct.Sender.html
  /// [`Receiver`]: struct.Receiver.html
  #[inline(always)]
  pub fn is_canceled(&self) -> bool {
    self.inner.is_canceled()
  }
}

impl<E> Drop for Sender<E> {
  #[inline]
  fn drop(&mut self) {
    self.inner.drop_tx();
  }
}

impl<E> Inner<E> {
  #[inline(always)]
  fn send_tick(&self) -> Result<(), SendTickError> {
    self
      .update(self.state_load(Relaxed), Acquire, Relaxed, |state| {
        let mut lock = *state & LOCK_MASK;
        if lock & COMPLETE != 0 {
          return Err(SendTickError::Canceled);
        }
        *state = (*state as isize >> LOCK_BITS) as usize;
        *state = state.wrapping_add(1);
        if *state == 0 {
          return Err(SendTickError::Overflow);
        }
        let rx_locked = if lock & RX_LOCK == 0 {
          lock |= RX_LOCK;
          true
        } else {
          false
        };
        *state <<= LOCK_BITS;
        *state |= lock;
        if rx_locked {
          Ok(Some(*state))
        } else {
          Ok(None)
        }
      })
      .map(|state| {
        state.map(|state| {
          unsafe { (*self.rx_task.get()).as_ref().map(|task| task.notify()) };
          self.update(state, Release, Relaxed, |state| {
            *state ^= RX_LOCK;
            Ok::<(), ()>(())
          })
        });
      })
  }

  #[inline(always)]
  pub fn send_err(&self, err: E) -> Result<(), E> {
    if self.is_canceled() {
      Err(err)
    } else {
      unsafe { *self.err.get() = Some(err) };
      Ok(())
    }
  }
}
