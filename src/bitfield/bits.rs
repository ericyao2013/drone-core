use core::fmt::Debug;
use core::mem::size_of;
use core::nonzero::Zeroable;
use core::ops::{BitAnd, BitOr, BitXor, Not, Shl, Shr, Sub};

/// Underlying integer for [`Bitfield`].
///
/// [`Bitfield`]: trait.Bitfield.html
pub trait Bits
where
  Self: Sized
    + Zeroable
    + Debug
    + Copy
    + PartialOrd
    + Not<Output = Self>
    + Sub<Output = Self>
    + BitOr<Output = Self>
    + BitXor<Output = Self>
    + BitAnd<Output = Self>
    + Shl<Self, Output = Self>
    + Shr<Self, Output = Self>,
{
  /// Converts `usize` to `Bits`.
  fn from_usize(bits: usize) -> Self;

  /// Returns the width of the type in bits.
  fn width() -> Self;

  /// Returns the value of one.
  fn one() -> Self;
}

macro_rules! bits {
  ($type:ty) => {
    impl Bits for $type {
      #[inline(always)]
      fn from_usize(bits: usize) -> Self {
        bits as $type
      }

      #[inline(always)]
      fn width() -> $type {
        size_of::<$type>() as $type * 8
      }

      #[inline(always)]
      fn one() -> $type {
        1
      }
    }
  }
}

bits!(u8);
bits!(u16);
bits!(u32);
bits!(u64);
