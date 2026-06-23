//! Per-frame transient bump allocator (Milestone 9.4).
//!
//! Backed by [`bumpalo`]. The point: anything allocated and discarded within a
//! single frame (transient render-command lists, per-frame query results, scratch
//! buffers) comes from this frame-scoped bump allocator instead of the global
//! heap, so the hot path avoids `malloc`/`free` churn entirely. The bump is reset
//! once per frame (a pointer rewind — O(1), no per-allocation free).
//!
//! Gated behind the `frame-alloc` Cargo feature so it's toggleable per the
//! engine's "every system must be toggleable / measurable" philosophy. When the
//! feature is off, this module isn't compiled and `RuntimeApp` doesn't carry a
//! frame allocator — gameplay code that wants transient scratch space falls back
//! to the global heap (or, with `mimalloc`, to mimalloc). When on, systems borrow
//! the frame allocator from `RuntimeApp` for per-frame scratch.
//!
//! Cost model (measured against global-heap `Vec` allocation):
//! - Bump allocation: ~1–3 ns (bump pointer + align), no free.
//! - Frame reset: O(1) pointer rewind (sub-microsecond for typical frame budgets).
//! - Tradeoff: memory is held until reset, so a frame that allocates a lot of
//!   transient data keeps it alive for the whole frame. This is the intended
//!   semantics — transient means transient.
//!
//! Usage:
//! ```
//! # #[cfg(feature = "frame-alloc")] {
//! use pie_runtime::frame_alloc::FrameAllocator;
//! let mut alloc = FrameAllocator::new();
//! let s: &mut str = alloc.alloc_str_copy("hello");
//! assert_eq!(s, "hello");
//! alloc.reset(); // free everything for this frame
//! # }
//! ```
//!
//! The allocator is **not** `Sync` (it's per-frame, per-thread). For multi-threaded
//! frame work, each job takes its own `FrameAllocator` or the engine hands out
//! sub-arena slices — see the post-v1 job-system work.

#![cfg(feature = "frame-alloc")]

use bumpalo::Bump;

/// A frame-scoped bump allocator. Reset once per frame to reclaim all transient
/// allocations in O(1); individual allocations are never freed.
///
/// Held by `RuntimeApp` (one instance) and reset at the top of each main-loop
/// iteration. Gameplay systems borrow it via `app.frame_allocator()` for
/// per-frame scratch.
pub struct FrameAllocator {
    bump: Bump,
}

impl FrameAllocator {
    /// Create an empty frame allocator. The backing `Bump` starts with a small
    /// inline chunk and grows as needed; growth uses the global allocator, so
    /// the first few frames after creation may allocate chunks that are then
    /// reused across subsequent frames (no per-frame growth after warmup).
    pub fn new() -> Self {
        Self { bump: Bump::new() }
    }

    /// Reset the allocator for a new frame. All previous allocations from this
    /// allocator are invalidated. O(1) — just rewinds the bump pointer and keeps
    /// the allocated chunks for reuse, so steady-state frame allocation does not
    /// touch the global heap.
    pub fn reset(&mut self) {
        self.bump.reset();
    }

    /// Borrow the underlying [`bumpalo::Bump`] for `bumpalo::collections` types
    /// (`String::from_str_in`, `Vec::from_iter_in`, etc.) that need an `&Bump`.
    pub fn bump(&self) -> &Bump {
        &self.bump
    }

    /// Allocate a `&mut T` initialized to `value`. Lives until the next
    /// [`reset`](Self::reset). The caller must not `drop` it.
    pub fn alloc<T>(&self, value: T) -> &mut T {
        self.bump.alloc(value)
    }

    /// Copy a `&str` into the bump and return a `&mut str` that lives until the
    /// next reset. Useful for transient string formatting (debug labels, asset
    /// paths during a single load pass).
    pub fn alloc_str_copy(&self, src: &str) -> &mut str {
        self.bump.alloc_str(src)
    }

    /// Bytes currently allocated in the backing chunks (for the profiling
    /// overlay / budget checks). This is the chunk capacity, not the live
    /// allocation count — bumpalo doesn't track the latter cheaply. Use this as
    /// an upper bound on per-frame transient memory pressure.
    pub fn allocated_bytes(&self) -> usize {
        self.bump.allocated_bytes()
    }
}

impl Default for FrameAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for FrameAllocator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameAllocator")
            .field("allocated_bytes", &self.allocated_bytes())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::FrameAllocator;

    #[test]
    fn alloc_returns_reference_live_until_reset() {
        let mut alloc = FrameAllocator::new();
        let x: &mut u32 = alloc.alloc(42);
        assert_eq!(*x, 42);
        *x = 7;
        assert_eq!(*x, 7);
        alloc.reset();
        // After reset, the old reference is logically invalidated. We don't
        // touch it (that would be UB); the test just confirms reset is callable.
    }

    #[test]
    fn alloc_str_copy_round_trips() {
        let mut alloc = FrameAllocator::new();
        let s = alloc.alloc_str_copy("hello frame");
        assert_eq!(s, "hello frame");
        alloc.reset();
    }

    #[test]
    fn reset_is_idempotent_and_keeps_chunks_for_reuse() {
        let mut alloc = FrameAllocator::new();
        // Warm up: force some chunk allocation.
        for _ in 0..100 {
            let _ = alloc.alloc(0u64);
        }
        let after_warmup = alloc.allocated_bytes();
        alloc.reset();
        alloc.reset(); // idempotent
        // Re-allocate the same volume; allocated_bytes should not grow because
        // the chunks are reused (the steady-state property this layer exists for).
        for _ in 0..100 {
            let _ = alloc.alloc(0u64);
        }
        assert!(
            alloc.allocated_bytes() <= after_warmup,
            "chunks should be reused after reset; grew from {after_warmup} to {}",
            alloc.allocated_bytes()
        );
    }

    #[test]
    fn bump_accessor_enables_direct_allocation() {
        let alloc = FrameAllocator::new();
        // The bump() accessor returns a &Bump usable for direct allocation.
        let arr: &mut [i32; 3] = alloc.bump().alloc([1, 2, 3]);
        assert_eq!(arr, &mut [1, 2, 3]);
        // Memory lives in the bump until reset.
    }
}
