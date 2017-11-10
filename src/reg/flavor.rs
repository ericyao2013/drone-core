/// Register flavor trait.
pub trait RegFlavor: Sized + Default {}

/// Marker trait for owned registers.
pub trait RegOwned: RegFlavor {}

/// Marker trait for shared registers.
pub trait RegShared: RegFlavor {}

/// Zero-sized marker type for **thread-unsafe** register bindings. "Ur" stands
/// for "Unique Register". Does implement `Send`, but not `Sync`, 'Clone', and
/// 'Copy'.
#[derive(Default)]
pub struct Ur;

impl !Sync for Ur {}
impl RegFlavor for Ur {}
impl RegOwned for Ur {}

/// Zero-sized marker type for **thread-safe** register bindings. "Sr" stands
/// for "Shared register". Does implement `Send` and `Sync`, but not 'Clone' and
/// 'Copy'.
#[derive(Default)]
pub struct Sr;

impl RegFlavor for Sr {}
impl RegOwned for Sr {}
impl RegShared for Sr {}

/// Zero-sized marker type for **thread-safe** register bindings. "Cr" stands
/// for "Copyable register". Does implement `Send`, `Sync`, 'Clone', and 'Copy'.
#[derive(Clone, Copy, Default)]
pub struct Cr;

impl RegFlavor for Cr {}
impl RegShared for Cr {}
