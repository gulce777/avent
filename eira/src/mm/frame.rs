//! Physical memory frames.
//!
//! A [`PhysFrame`] is a page-aligned [`PhysAddr`] that represents a single
//! physical page of memory. The page is a const generic parameter so that
//! the type system distinguishes 4 KiB frames from 2 MiB large frames and
//! 1 GiB huge frames. Mixing them is a compile-time error.

use super::addr::{HUGE_PAGE_SIZE, LARGE_PAGE_SIZE, PAGE_SIZE, PhysAddr};
use crate::mm::FrameAllocator;
use core::fmt;
use core::marker::PhantomData;

/// A page-aligned physical memory frame of size `S` bytes.
///
/// The const generic `S` must be a power of two. Use the type aliases
/// [`Frame4K`], [`Frame2M`] and [`Frame1G`] for the common sizes.
///
/// A `PhysFrame` is guaranteed to be aligned to `S` bytes and
/// to not exceed the 52-bit physical address space.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PhysFrame<const S: usize = PAGE_SIZE> {
    /// The base address of this frame. Always aligned to `S`.
    base: PhysAddr,
    _size: PhantomData<[u8; S]>,
}

/// A standard 4 KiB physical frame.
pub type Frame4K = PhysFrame<{ PAGE_SIZE }>;
/// A 2 MiB large physical frame.
pub type Frame2M = PhysFrame<{ LARGE_PAGE_SIZE }>;
/// A 1 GiB huge physical frame.
pub type Frame1G = PhysFrame<{ HUGE_PAGE_SIZE }>;

impl<const S: usize> PhysFrame<S> {
    pub const SIZE: usize = S;

    /// Construct a `PhysFrame` from a base address.
    ///
    /// Returns `None` if `base` is not aligned to `S` bytes.
    ///
    /// # Panics
    ///
    /// Panics at compile time if `S` is not a power of two.
    #[inline]
    pub const fn from_base(base: PhysAddr) -> Option<Self> {
        const { assert!(S.is_power_of_two(), "frame size must be a power of two") };

        if base.is_aligned(S) {
            Some(Self {
                base,
                _size: PhantomData,
            })
        } else {
            None
        }
    }

    /// Returns the frame that contains `addr`, by aligning down to `S`.
    #[inline]
    pub const fn containing(addr: PhysAddr) -> Self {
        const { assert!(S.is_power_of_two(), "frame size must be a power of two") };

        Self {
            base: addr.align_down(S),
            _size: PhantomData,
        }
    }

    /// Returns the base address of this frame.
    #[inline]
    pub const fn base(self) -> PhysAddr {
        self.base
    }

    /// Returns the exclusive end address of this frame (`base + S`).
    #[inline]
    pub const fn end(self) -> PhysAddr {
        // SAFETY: base + S cannot overflow base <= PhysAddr::MAX - S because
        // we only ever construct frames within the 52-bit space.
        unsafe { PhysAddr::new_unchecked(self.base.as_usize() + S) }
    }

    /// Returns the frame index of this frame.
    ///
    /// Frame index 0 is the frame at physical address 0.
    #[inline]
    pub const fn index(self) -> usize {
        self.base.as_usize() / S
    }

    /// Construct a `PhysFrame` from a frame index.
    ///
    /// Returns `None` if the resulting address exceeds the 52-bit space.
    #[inline]
    pub const fn from_index(index: usize) -> Option<Self> {
        match index.checked_mul(S) {
            Some(addr) => match PhysAddr::new(addr) {
                Some(base) => Some(Self {
                    base,
                    _size: PhantomData,
                }),
                None => None,
            },
            None => None,
        }
    }
}

impl<const S: usize> fmt::Debug for PhysFrame<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PhysFrame<{}K>({:#018x})",
            S / 1024,
            self.base.as_usize()
        )
    }
}

/// An owned, non-`Copy` handle to a single physical frame.
///
/// Guarantees that each allocated frame has exactly one owner at any time.
/// Double-free is a compile-time error. `free` consumes `self`, so the
/// same `OwnedFrame` cannot be freed twice.
///
/// Dropping an `OwnedFrame` without calling [`free`](OwnedFrame::free)
/// will panic.
#[derive(Debug)]
pub struct OwnedFrame {
    inner: PhysFrame,
}

impl OwnedFrame {
    /// Wrap a raw `PhysFrame` in an `OwnedFrame`.
    ///
    /// Only the allocator should call this, hence `pub(crate)`.
    pub(crate) fn new(frame: PhysFrame) -> Self {
        Self { inner: frame }
    }

    /// Returns the base physical address of this frame.
    #[inline]
    pub fn base(&self) -> PhysAddr {
        self.inner.base()
    }

    /// Return this frame to `allocator`, consumes the `OwnedFrame`.
    ///
    /// After this call the frame may be handed out again by a future
    /// [`allocate`](crate::mm::allocate) call.
    pub fn free(self, allocator: &mut impl FrameAllocator) {
        let this = core::mem::ManuallyDrop::new(self);

        unsafe { allocator.deallocate(this.inner) };
    }
}

impl Drop for OwnedFrame {
    fn drop(&mut self) {
        // Reaching here is always a bug. The frame was neither freed nor
        // leaked via `ManuallyDrop`.
        panic!("owned frame dropped without being freed. {:?}", self.inner);
    }
}

/// An inclusive range of physical frames `[start, end]`.
///
/// Both `start` and `end` are inclusive. An empty range is one where
/// `start > end`.
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct FrameRange<const S: usize = PAGE_SIZE> {
    start: PhysFrame<S>,
    end: PhysFrame<S>, // inclusive
}

impl<const S: usize> FrameRange<S> {
    /// Construct a `FrameRange` from an inclusive `[start, end]` pair.
    #[inline]
    pub const fn new(start: PhysFrame<S>, end: PhysFrame<S>) -> Self {
        Self { start, end }
    }

    /// Construct a `FrameRange` from a base address and a byte length.
    ///
    /// Returns `None` if the range would overflow or either address is
    /// not representable in 52 bits.
    #[inline]
    pub const fn from_addr_len(base: PhysAddr, len: usize) -> Option<Self> {
        if len == 0 {
            return None;
        }
        let end_addr = match base.checked_add(len - 1) {
            Some(a) => a,
            None => return None,
        };
        Some(Self {
            start: PhysFrame::containing(base),
            end: PhysFrame::containing(end_addr),
        })
    }

    /// Returns the first frame in the range.
    #[inline]
    pub const fn start(self) -> PhysFrame<S> {
        self.start
    }

    /// Returns the last (inclusive) frame in the range.
    #[inline]
    pub const fn end(self) -> PhysFrame<S> {
        self.end
    }

    /// Returns `true` if the range contains no frames.
    #[inline]
    pub const fn is_empty(self) -> bool {
        self.start.index() > self.end.index()
    }

    /// Returns the number of frames in this range.
    #[inline]
    pub const fn len(self) -> usize {
        if self.is_empty() {
            0
        } else {
            self.end.index() - self.start.index() + 1
        }
    }

    /// Returns `true` if `frame` is contained in this range.
    #[inline]
    pub const fn contains(self, frame: PhysFrame<S>) -> bool {
        frame.index() >= self.start.index() && frame.index() <= self.end.index()
    }
}

impl<const S: usize> Iterator for FrameRange<S> {
    type Item = PhysFrame<S>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.start <= self.end {
            let frame = self.start;
            self.start = PhysFrame::from_index(self.start.index() + 1)?;
            Some(frame)
        } else {
            None
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl<const S: usize> ExactSizeIterator for FrameRange<S> {}

impl<const S: usize> fmt::Debug for FrameRange<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FrameRange({:?}..={:?})", self.start, self.end)
    }
}
