#![warn(clippy::pedantic)]
#![no_std]
//! Allocate fixed-size arrays directly on the heap.
//!
//! # The Problem
//!
//! In Rust, `Box::new([T; N])` allocates on the stack first, then moves to the
//! heap. For large arrays, this can overflow the stack. Rust's optimizer
//! should handle this in optimized builds, but not in debug.
//!
//! # The Solution
//!
//! This crate allocates `[T; N]` directly on the heap, skipping the intermediate stack allocation.
//!
//! # Example
//!
//! ```no_run
//! const SIZE: usize = 1_000_000_000;
//! // This would overflow the stack in debug mode:
//! // let arr = Box::new([0u8; SIZE]);
//!
//! // This works everywhere:
//! let arr: Box<[u8; SIZE]> = heap_arr::new_copied(&0u8);
//! assert_eq!(arr[0], 0);
//! ```
//!
//! # API
//!
//! - [`new_copied`] – Allocate and fill by copying a value (for `T: Copy`)
//! - [`new_cloned`] – Allocate and fill by cloning a value (for `T: Clone`)
//! - [`new_default`] – Allocate and fill with `T::default()`
//! - [`from_fn`] – Initialize via a closure
//! - [`try_from_fn`] – Initialize with a fallible closure
//! - [`uninit`] – Allocate uninitialized memory
//! - [`BoxExt::assume_init`] – Transmute uninitialized memory (unsafe)
//!
//! All functions return `Box<[T; N]>` compatible with standard library code.
//!
//! # Features
//!
//! - `#![no_std]` compatible (uses `alloc` only)
//! - Panic-safe initialization with automatic cleanup on drop

mod box_ext;
mod guard;

use core::mem::MaybeUninit;

use alloc::boxed::Box;

pub use crate::box_ext::BoxExt;
use crate::guard::Guard;

extern crate alloc;

/// Allocates `[T; N]` directly on the heap and initializes its entries to
/// `initial` by copying it. For `T` that are not `Copy`, see [`new_cloned`].
///
/// # Examples
/// ```
/// const LEN: usize = 1000;
///
/// let mut arr: Box::<[usize; LEN]> = heap_arr::new_copied(&123);
/// assert_eq!(arr[LEN - 1], 123);
/// ```
#[must_use]
pub fn new_copied<T, const N: usize>(initial: &T) -> Box<[T; N]>
where
    T: Copy,
{
    // FIXME: This can be made better once the `maybe_uninit_fill` feature
    // gets merged. https://github.com/rust-lang/rust/issues/117428

    // No guard needed as `T: Copy` guarantees that the initialization
    // is infallible
    let mut array = uninit();
    for v in &mut *array {
        v.write(*initial);
    }
    unsafe { array.assume_init() }
}

/// Allocates `[T; N]` directly on the heap and initializes its entries to
/// `initial` by cloning it. For `T` that are `Copy`, see [`new_copied`].
///
/// # Examples
/// ```
/// const LEN: usize = 1000;
///
/// let mut arr: Box::<[usize; LEN]> = heap_arr::new_cloned(&123);
/// assert_eq!(arr[LEN - 1], 123);
/// ```
#[must_use]
pub fn new_cloned<T, const N: usize>(initial: &T) -> Box<[T; N]>
where
    T: Clone,
{
    // FIXME: This can be made better once the `maybe_uninit_fill` feature
    // gets merged. https://github.com/rust-lang/rust/issues/117428

    let mut array = uninit();
    let mut guard = Guard::new(&mut array);
    for _ in 0..N {
        unsafe { guard.push_unchecked(initial.clone()) };
    }
    unsafe { guard.finalize() };

    unsafe { array.assume_init() }
}

/// Allocates `[T; N]` directly on the heap and initializes its entries to
/// `T::default()`.
///
/// # Examples
/// ```
/// const LEN: usize = 1000;
///
/// let mut arr: Box::<[bool; LEN]> = heap_arr::new_default();
/// assert_eq!(arr[LEN - 1], false);
/// ```
#[must_use]
pub fn new_default<T, const N: usize>() -> Box<[T; N]>
where
    T: Default,
{
    let mut array = uninit();
    let mut guard = Guard::new(&mut array);
    for _ in 0..N {
        unsafe { guard.push_unchecked(T::default()) };
    }
    unsafe { guard.finalize() };

    unsafe { array.assume_init() }
}

/// Allocates `[T; N]` directly on the heap and initializes its entries to
/// values produced by calling `f` with the index of the entry as argument
/// while walking forward through the array.
///
/// # Examples
/// ```
/// const LEN: usize = 1000;
///
/// let mut arr: Box::<[bool; LEN]> = heap_arr::from_fn(|i| i % 2 == 0);
/// assert_eq!(arr[0], true);
/// assert_eq!(arr[1], false);
/// assert_eq!(arr[2], true);
/// ```
#[must_use]
pub fn from_fn<T, const N: usize, F>(mut f: F) -> Box<[T; N]>
where
    F: FnMut(usize) -> T,
{
    let mut array = uninit();
    let mut guard = Guard::new(&mut array);
    for i in 0..N {
        unsafe { guard.push_unchecked(f(i)) };
    }
    unsafe { guard.finalize() };

    unsafe { array.assume_init() }
}

/// Allocates `[T; N]` directly on the heap and initializes its entries to
/// values produced by calling `f` with the index of the entry as argument
/// while walking forward through the array.
///
/// # Examples
/// ```
/// const LEN: usize = 1000;
///
/// let mut xs = heap_arr::try_from_fn::<usize, (), LEN, _>(|i| Ok(i));
/// assert!(xs.is_ok());
///
/// let mut ys = heap_arr::try_from_fn::<usize, (), LEN, _>(|i| Err(()));
/// assert!(ys.is_err());
/// ```
#[allow(clippy::missing_errors_doc)]
pub fn try_from_fn<T, E, const N: usize, F>(mut f: F) -> Result<Box<[T; N]>, E>
where
    F: FnMut(usize) -> Result<T, E>,
{
    // FIXME: This can be made better once the `Try` trait becomes stable.
    // https://github.com/rust-lang/rust/issues/84277

    let mut array = uninit();
    let mut guard = Guard::new(&mut array);
    for i in 0..N {
        let v = f(i)?;
        unsafe { guard.push_unchecked(v) };
    }
    unsafe { guard.finalize() };

    unsafe { Ok(array.assume_init()) }
}

/// Allocates `[T; N]` directly on the heap and leaves its entries
/// uninitialized.
///
/// # Examples
/// ```
/// use heap_arr::BoxExt;
///
/// let mut arr = heap_arr::uninit::<u64, 1000>();
/// // SAFETY: We must ensure to initialize all entries before using the array
/// for v in &mut *arr {
///     v.write(11);
/// }
///
/// let arr = unsafe { arr.assume_init() };
/// ```
#[must_use]
pub fn uninit<T, const N: usize>() -> Box<[MaybeUninit<T>; N]> {
    // Safety: MaybeUninit<[T; N]> can safely be turned into [MaybeUninit<T>; N]
    unsafe { Box::new_uninit().assume_init() }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use alloc::string::String;
    use core::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn new_copied() {
        const LEN: usize = 1024;

        let arr = super::new_copied::<Option<bool>, LEN>(&Some(true));
        assert_eq!(arr.len(), LEN);
        assert!(arr.iter().all(|value| *value == Some(true)));
    }

    #[test]
    fn new_cloned() {
        const LEN: usize = 16;

        let arr = super::new_cloned::<String, LEN>(&String::from("value"));
        assert_eq!(arr.len(), LEN);
        assert!(arr.iter().all(|value| value == "value"));
    }

    // Miri does not enforce native stack limits.
    #[cfg(not(miri))]
    mod small_stack {
        use super::std::thread;
        use core::hint::black_box;

        // The array exceeds the thread's stack; stack allocation overflows in debug.
        const LEN: usize = 1024 * 1024;

        fn run(name: &str, f: impl FnOnce() + Send + 'static) {
            thread::Builder::new()
                .name(name.into())
                .stack_size(64 * 1024)
                .spawn(f)
                .unwrap()
                .join()
                .unwrap();
        }

        #[test]
        fn new_copied() {
            run("new_copied", || {
                let arr = black_box(crate::new_copied::<u8, LEN>(&7));
                assert_eq!(arr[LEN - 1], 7);
            });
        }

        #[test]
        fn new_cloned() {
            run("new_cloned", || {
                let arr = black_box(crate::new_cloned::<u8, LEN>(&7));
                assert_eq!(arr[LEN - 1], 7);
            });
        }

        #[test]
        fn new_default() {
            run("new_default", || {
                let arr = black_box(crate::new_default::<u8, LEN>());
                assert_eq!(arr[LEN - 1], 0);
            });
        }

        #[test]
        fn from_fn() {
            run("from_fn", || {
                let arr = black_box(crate::from_fn::<u8, LEN, _>(|_| 7));
                assert_eq!(arr[LEN - 1], 7);
            });
        }

        #[test]
        fn try_from_fn() {
            run("try_from_fn", || {
                let arr = black_box(crate::try_from_fn::<u8, (), LEN, _>(|_| Ok(7)).unwrap());
                assert_eq!(arr[LEN - 1], 7);
            });
        }

        #[test]
        fn uninit() {
            run("uninit", || {
                let arr = black_box(crate::uninit::<u8, LEN>());
                assert_eq!(arr.len(), LEN);
            });
        }
    }

    #[test]
    fn default() {
        let arr = super::new_default::<Option<bool>, 1000>();
        assert_eq!(arr.len(), 1000);
        assert_eq!(arr[999], None);
    }

    #[test]
    fn from_fn() {
        let arr = super::from_fn::<u64, 1000, _>(|i| i as u64);
        assert_eq!(arr.len(), 1000);
        assert_eq!(arr[999], 999);
    }

    #[test]
    fn try_from_fn_partial_drop_cleanup() {
        struct DropCounter {
            counter: &'static AtomicUsize,
        }

        impl Drop for DropCounter {
            fn drop(&mut self) {
                self.counter.fetch_add(1, Ordering::SeqCst);
            }
        }

        static DROPS: AtomicUsize = AtomicUsize::new(0);
        let result = crate::try_from_fn::<_, _, 1024, _>(|i| {
            if i < 2 {
                Ok(DropCounter { counter: &DROPS })
            } else {
                Err(())
            }
        });
        assert!(result.is_err());

        assert_eq!(DROPS.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn uninit() {
        const LEN: usize = 1024;
        let arr = super::uninit::<u64, LEN>();
        assert_eq!(arr.len(), LEN);
        core::hint::black_box(arr);
    }
}
