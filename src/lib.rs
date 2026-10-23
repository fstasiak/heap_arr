#![warn(clippy::pedantic)]
#![no_std]
//! Allocate fixed-size arrays directly on the heap without first constructing
//! the entire array on the stack.
//!
//! `Box::new([value; N])` can construct an intermediate array on the stack,
//! potentially overflowing it for large `N`. Optimizations may eliminate this
//! temporary, but that behavior is not guaranteed. This crate initializes the
//! heap allocation one element at a time. Initializers can still use stack
//! space of their own.
//!
//! # Example
//!
//! ```no_run
//! const SIZE: usize = 1_000_000_000;
//! // This can overflow the stack in debug mode:
//! // let arr = Box::new([0u8; SIZE]);
//!
//! // Requires enough heap memory for the array:
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
//! - [`BoxExt::assume_init`] – Convert a fully initialized allocation (unsafe)
//!
//! Initialized arrays use `Box<[T; N]>`. [`try_from_fn`] wraps this in a
//! `Result`, and [`uninit`] returns `Box<[MaybeUninit<T>; N]>`.
//!
//! # Errors and panics
//!
//! [`try_from_fn`] returns errors from its initializer; allocation failures are
//! handled by the allocator and allocation error handler, not returned as `E`.
//! The safe constructors clean up initialized elements on an error or during
//! panic unwinding. Destructors are not guaranteed to run if the process aborts.
//! Manual initialization through [`uninit`] requires the caller to manage cleanup.
//!
//! # `no_std`
//!
//! The crate depends only on `alloc`. The final application must provide a
//! global allocator and any allocation error handling required by its target.

extern crate alloc;

mod box_ext;
mod guard;

pub use crate::box_ext::BoxExt;

use crate::guard::Guard;
use alloc::boxed::Box;
use core::mem::MaybeUninit;

/// Allocates `[T; N]` directly on the heap and initializes its entries to
/// `initial` by copying it. For `T` that are not `Copy`, see [`new_cloned`].
/// This does not call `T::clone`.
///
/// # Examples
/// ```
/// let array: Box<[u8; 4]> = heap_arr::new_copied(&7);
/// assert_eq!(*array, [7; 4]);
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
    // SAFETY: Every element has been initialized by copying a valid T.
    unsafe { array.assume_init() }
}

/// Allocates `[T; N]` directly on the heap and initializes its entries to
/// `initial` by cloning it. For `T` that are `Copy`, see [`new_copied`].
/// Calls `initial.clone()` once per element, or zero times when `N = 0`.
///
/// # Panics
/// Propagates panics from `Clone::clone`. Initialized elements are dropped
/// during unwinding; `initial` remains owned by the caller.
///
/// # Examples
/// ```
/// let value = String::from("hello");
/// let array: Box<[String; 3]> = heap_arr::new_cloned(&value);
/// assert!(array.iter().all(|element| element == "hello"));
/// assert_eq!(value, "hello");
/// ```
#[must_use]
pub fn new_cloned<T, const N: usize>(initial: &T) -> Box<[T; N]>
where
    T: Clone,
{
    // A direct loop avoids closure adapter calls in debug builds.
    let mut guard = Guard::new();
    for _ in 0..N {
        guard.push(initial.clone());
    }
    guard.finish()
}

/// Allocates `[T; N]` directly on the heap and initializes its entries to
/// `T::default()`.
/// Calls `T::default()` once per element, or zero times when `N = 0`.
///
/// # Panics
/// Propagates panics from `Default::default`. Initialized elements are dropped
/// during unwinding.
///
/// # Examples
/// ```
/// let array: Box<[Option<u32>; 3]> = heap_arr::new_default();
/// assert_eq!(*array, [None; 3]);
/// ```
#[must_use]
pub fn new_default<T, const N: usize>() -> Box<[T; N]>
where
    T: Default,
{
    let mut guard = Guard::new();
    for _ in 0..N {
        guard.push(T::default());
    }
    guard.finish()
}

/// Allocates an array on the heap and initializes each element with `f(index)`.
/// Calls `f` in ascending index order, once per element unless a call panics.
/// For `N = 0`, `f` is not called.
///
/// # Panics
/// Propagates panics from `f`. Initialized elements are dropped during unwinding.
///
/// # Examples
/// ```
/// let array: Box<[usize; 4]> = heap_arr::from_fn(|i| i * i);
/// assert_eq!(*array, [0, 1, 4, 9]);
/// ```
#[must_use]
pub fn from_fn<T, const N: usize, F>(mut f: F) -> Box<[T; N]>
where
    F: FnMut(usize) -> T,
{
    let mut guard = Guard::new();
    for i in 0..N {
        guard.push(f(i));
    }
    guard.finish()
}

/// Allocates an array on the heap using a fallible initializer.
/// Calls `f(index)` in ascending index order, stopping at the first error or panic.
/// For `N = 0`, returns an empty array without calling `f`.
///
/// # Errors
/// Returns the first error from `f` unchanged, without calling it for later
/// indices. Elements initialized before the error are dropped.
/// Allocation failures are not returned through this `Result`.
///
/// # Panics
/// Propagates panics from `f`. Initialized elements are dropped during unwinding.
///
/// # Examples
/// ```
/// let result = heap_arr::try_from_fn::<usize, _, 4, _>(|i| {
///     if i == 2 { Err(i) } else { Ok(i * i) }
/// });
/// assert_eq!(result.unwrap_err(), 2);
/// ```
pub fn try_from_fn<T, E, const N: usize, F>(mut f: F) -> Result<Box<[T; N]>, E>
where
    F: FnMut(usize) -> Result<T, E>,
{
    let mut guard = Guard::new();
    for i in 0..N {
        guard.push(f(i)?);
    }
    Ok(guard.finish())
}

/// Allocates `[T; N]` directly on the heap and leaves its entries
/// uninitialized.
///
/// Dropping this box does not drop any `T` values written into it. If manual
/// initialization fails or panics, the caller must clean up initialized values
/// to avoid leaking their resources. Prefer [`from_fn`] or [`try_from_fn`] when
/// possible; both manage partial initialization automatically.
///
/// # Examples
/// ```
/// use heap_arr::BoxExt;
///
/// let mut array = heap_arr::uninit::<u64, 3>();
/// for element in &mut *array {
///     element.write(11);
/// }
///
/// // SAFETY: Every element has been initialized.
/// let array = unsafe { array.assume_init() };
/// assert_eq!(*array, [11; 3]);
/// ```
#[must_use]
pub fn uninit<T, const N: usize>() -> Box<[MaybeUninit<T>; N]> {
    // SAFETY: Uninitialized memory is valid for [MaybeUninit<T>; N].
    // No initialized T values are created here.
    unsafe { Box::new_uninit().assume_init() }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use alloc::boxed::Box;
    use alloc::rc::Rc;
    use alloc::string::String;
    use core::cell::Cell;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use std::panic::{catch_unwind, AssertUnwindSafe};

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

    #[test]
    fn new_cloned_cleanup_on_panic() {
        const LEN: usize = 5;

        struct CloneTracker<'a> {
            calls: &'a Cell<usize>,
            drops: &'a Cell<usize>,
            panic_at: usize,
        }

        impl Clone for CloneTracker<'_> {
            fn clone(&self) -> Self {
                let index = self.calls.get();
                self.calls.set(index + 1);
                assert!(index != self.panic_at, "intentional clone panic");
                Self { ..*self }
            }
        }

        impl Drop for CloneTracker<'_> {
            fn drop(&mut self) {
                self.drops.set(self.drops.get() + 1);
            }
        }

        for panic_at in 0..LEN {
            let calls = Cell::new(0);
            let drops = Cell::new(0);
            let initial = CloneTracker {
                calls: &calls,
                drops: &drops,
                panic_at,
            };
            let result = catch_unwind(AssertUnwindSafe(|| {
                let _ = crate::new_cloned::<_, LEN>(&initial);
            }));

            let panic = result.expect_err("clone should panic");
            assert_eq!(
                panic.downcast_ref::<&str>(),
                Some(&"intentional clone panic")
            );
            assert_eq!(calls.get(), panic_at + 1);
            assert_eq!(drops.get(), panic_at);
            drop(initial);
            assert_eq!(drops.get(), panic_at + 1);
        }
    }

    #[test]
    fn new_default_cleanup() {
        const LEN: usize = 5;
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        static DROPS: AtomicUsize = AtomicUsize::new(0);
        static PANIC_AT: AtomicUsize = AtomicUsize::new(0);

        struct DefaultTracker;

        impl Default for DefaultTracker {
            fn default() -> Self {
                let index = CALLS.fetch_add(1, Ordering::SeqCst);
                assert!(
                    index != PANIC_AT.load(Ordering::SeqCst),
                    "intentional default panic"
                );
                Self
            }
        }

        impl Drop for DefaultTracker {
            fn drop(&mut self) {
                DROPS.fetch_add(1, Ordering::SeqCst);
            }
        }

        for panic_at in 0..=LEN {
            CALLS.store(0, Ordering::SeqCst);
            DROPS.store(0, Ordering::SeqCst);
            PANIC_AT.store(panic_at, Ordering::SeqCst);
            match catch_unwind(crate::new_default::<DefaultTracker, LEN>) {
                Ok(array) => {
                    assert_eq!(panic_at, LEN);
                    assert_eq!(CALLS.load(Ordering::SeqCst), LEN);
                    assert_eq!(DROPS.load(Ordering::SeqCst), 0);
                    drop(array);
                    assert_eq!(DROPS.load(Ordering::SeqCst), LEN);
                }
                Err(panic) => {
                    assert!(panic_at < LEN);
                    assert_eq!(
                        panic.downcast_ref::<&str>(),
                        Some(&"intentional default panic")
                    );
                    assert_eq!(CALLS.load(Ordering::SeqCst), panic_at + 1);
                    assert_eq!(DROPS.load(Ordering::SeqCst), panic_at);
                }
            }
        }
    }

    #[test]
    fn initialized_arrays_release_resources() {
        fn check(make: impl FnOnce(&Rc<()>) -> Box<[Rc<()>; 4]>) {
            let initial = Rc::new(());
            let array = make(&initial);
            assert_eq!(Rc::strong_count(&initial), 5);
            drop(array);
            assert_eq!(Rc::strong_count(&initial), 1);
        }

        check(crate::new_cloned);
        check(|initial| crate::from_fn(|_| Rc::clone(initial)));
        check(|initial| crate::try_from_fn::<_, (), 4, _>(|_| Ok(Rc::clone(initial))).unwrap());
    }

    #[test]
    fn empty_arrays_do_not_initialize_elements() {
        struct MustNotInitialize;

        impl Clone for MustNotInitialize {
            fn clone(&self) -> Self {
                panic!("empty arrays must not call Clone");
            }
        }

        impl Default for MustNotInitialize {
            fn default() -> Self {
                panic!("empty arrays must not call Default");
            }
        }

        assert!(crate::new_copied::<u8, 0>(&7).is_empty());
        assert!(crate::new_cloned::<_, 0>(&MustNotInitialize).is_empty());
        assert!(crate::new_default::<MustNotInitialize, 0>().is_empty());
        assert!(crate::from_fn::<String, 0, _>(|_| panic!("initializer must not run")).is_empty());
        assert!(
            crate::try_from_fn::<String, (), 0, _>(|_| panic!("initializer must not run"))
                .unwrap()
                .is_empty()
        );
        assert!(crate::uninit::<String, 0>().is_empty());
    }

    #[test]
    fn constructors_preserve_alignment() {
        #[repr(align(256))]
        #[derive(Clone, Copy, Default)]
        struct Aligned(u8);

        fn check(array: &[Aligned; 3], expected: u8) {
            assert!(array.as_ptr().is_aligned());
            assert!(array.iter().all(|value| value.0 == expected));
        }

        check(&crate::new_copied(&Aligned(7)), 7);
        check(&crate::new_cloned(&Aligned(7)), 7);
        check(&crate::new_default(), 0);
        check(&crate::from_fn(|_| Aligned(7)), 7);
        check(
            &crate::try_from_fn::<_, (), 3, _>(|_| Ok(Aligned(7))).unwrap(),
            7,
        );
        let array = crate::uninit::<Aligned, 3>();
        assert!(array.as_ptr().is_aligned());
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
        const LEN: usize = 5;

        struct DropCounter<'a>(&'a Cell<usize>);

        impl Drop for DropCounter<'_> {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }

        for fail_at in 0..LEN {
            let drops = Cell::new(0);
            let mut calls = 0;
            let result = crate::try_from_fn::<_, _, LEN, _>(|index| {
                calls += 1;
                if index == fail_at {
                    Err(index)
                } else {
                    Ok(DropCounter(&drops))
                }
            });

            assert_eq!(result.err(), Some(fail_at));
            assert_eq!(calls, fail_at + 1);
            assert_eq!(drops.get(), fail_at);
        }
    }

    #[test]
    fn uninit() {
        const LEN: usize = 1024;
        let arr = super::uninit::<u64, LEN>();
        assert_eq!(arr.len(), LEN);
        core::hint::black_box(arr);
    }
}
