#![warn(clippy::pedantic)]
#![no_std]

mod guard;

use core::mem::{self, MaybeUninit};

use alloc::boxed::Box;

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
    unsafe { assume_init(array) }
}

/// Allocates `[T; N]` directly on the heap and initializes its entries to
/// `initial` by cloning it. For `T` that are not `Clone`, see [`new_copied`].
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

    unsafe { assume_init(array) }
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

    unsafe { assume_init(array) }
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

    unsafe { assume_init(array) }
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

    unsafe { Ok(assume_init(array)) }
}

/// Allocates `[T; N]` directly on the heap and leaves its entries
/// uninitialized.
///
/// # Examples
/// ```
/// let mut arr = heap_arr::uninit::<u64, 1000>();
/// // SAFETY: We must ensure to initialize all entries before using the array
/// for v in &mut *arr {
///     unsafe { v.write(11) };
/// }
///
/// let arr = unsafe { heap_arr::assume_init(arr) };
/// ```
#[must_use]
pub fn uninit<T, const N: usize>() -> Box<[MaybeUninit<T>; N]> {
    // Safety: MaybeUninit<[T; N]> can safely be turned into [MaybeUninit<T>; N]
    unsafe { Box::new_uninit().assume_init() }
}

/// A helper function to transmute `Box<[MaybeUninit<T>; N]>` into
/// `Box<[T; N]>`.
///
/// # Safety
/// The caller must ensure that all entries of the input array are initialized
///
/// # Examples
/// ```
/// use std::mem::MaybeUninit;
///
/// let mut arr: Box<[MaybeUninit<u64>; 1000]> = heap_arr::uninit();
///
/// // SAFETY: We must ensure to initialize all entries before using the array
/// for v in &mut *arr {
///     unsafe { v.write(11) };
/// }
///
/// // All entries of `arr` are initialized at this point, so it's safe to transmute it
/// let arr = unsafe { heap_arr::assume_init(arr) };
/// ```
#[allow(clippy::unnecessary_box_returns)]
#[must_use]
pub unsafe fn assume_init<T, const N: usize>(arr: Box<[MaybeUninit<T>; N]>) -> Box<[T; N]> {
    // Safety: `Box<[MaybeUninit<T>; N]>` and `Box<[T; N]>` have the same
    // layout, and the caller ensures all entries of the former are initialized
    unsafe { mem::transmute(arr) }
}

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    #[cfg(miri)]
    fn new_cloned() {
        const LEN: usize = 1024;

        let arr = super::new_cloned::<Option<bool>, LEN>(&Some(true));
        assert_eq!(arr.len(), LEN);
        assert_eq!(arr[LEN - 1], Some(true));
    }

    #[test]
    #[cfg(not(miri))]
    fn new_cloned() {
        // If we weren't allocating directly on the heap, this would cause
        // a stack overflow

        const LEN: usize = 1024 * 1024 * 1024; // 1Gi

        let arr = super::new_copied::<Option<bool>, LEN>(&Some(true));
        assert_eq!(arr.len(), LEN);
        assert_eq!(arr[LEN - 1], Some(true));
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
        const LEN: usize = 1024 * 1024 * 1024; // 1Gi
        let arr = super::uninit::<u64, LEN>();
        core::hint::black_box(arr);
    }
}
