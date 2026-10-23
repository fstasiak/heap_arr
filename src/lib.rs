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
