#![warn(clippy::pedantic)]
#![no_std]

use core::mem::{self, MaybeUninit};

use alloc::{
    alloc::{handle_alloc_error, Layout, LayoutError},
    boxed::Box,
};

extern crate alloc;

/// Allocates `[T; N]` on the heap and initializes its entries to `initial`.
///
/// For now, this function uses the global allocator. This will change once the
/// [`Allocator`](core::alloc::Allocator) trait becomes stable.
///
/// # Safety
/// The result of this function is undefined if `mem::size_of::<T>() == 0`, or
/// if `N == 0`
///
/// # Errors
/// See [`Layout::array`]
pub unsafe fn new<T, const N: usize>(initial: &T) -> Result<Box<[T; N]>, LayoutError>
where
    T: Clone,
{
    let mut arr = new_uninit::<T, N>()?;
    for v in arr.as_mut() {
        v.write(initial.clone());
    }

    Ok(mem::transmute(arr))
}

/// Allocates `[T; N]` on the heap and initializes its entries to `T::default()`.
///
/// For now, this function uses the global allocator. This will change once the
/// [`Allocator`](core::alloc::Allocator) trait becomes stable.
///
/// # Safety
/// The result of this function is undefined if `mem::size_of::<T>() == 0`, or
/// if `N == 0`
///
/// # Errors
/// See [`Layout::array`]
pub unsafe fn new_default<T, const N: usize>() -> Result<Box<[T; N]>, LayoutError>
where
    T: Default,
{
    let mut arr = new_uninit::<T, N>()?;
    for v in arr.as_mut() {
        v.write(T::default());
    }

    Ok(mem::transmute(arr))
}

/// Allocates `[T; N]` on the heap.
///
/// For now, this function uses the global allocator. This will change once the
/// [`Allocator`](core::alloc::Allocator) trait becomes stable.
///
/// # Safety
/// The result of this function is undefined if `mem::size_of::<T>() == 0`, or
/// if `N == 0`
///
/// # Errors
/// See [`Layout::array`]
///
/// # Examples
/// ```
/// use std::mem;
///
/// unsafe {
///     const LEN: usize = 1024 * 1024 * 1024;
///
///     let mut arr = heap_arr::new_uninit::<usize, LEN>().unwrap();
///     for (i, v) in arr.as_mut().iter_mut().enumerate() {
///         v.write(i);
///     }
///
///     let arr: Box::<[usize; LEN]> = mem::transmute(arr);
/// }
/// ```
pub unsafe fn new_uninit<T, const N: usize>() -> Result<Box<[MaybeUninit<T>; N]>, LayoutError> {
    let layout = Layout::array::<T>(N)?;
    let ptr = alloc::alloc::alloc(layout);

    if ptr.is_null() {
        handle_alloc_error(layout);
    }
    Ok(Box::from_raw(ptr.cast()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_new() {
        let arr = unsafe { super::new::<Option<bool>, 1_000>(&Some(false)).unwrap() };
        assert_eq!(arr.len(), 1_000);
        assert_eq!(arr[999], Some(false));
    }

    #[test]
    fn test_default() {
        let arr = unsafe { super::new_default::<Option<bool>, 1_000>().unwrap() };
        assert_eq!(arr.len(), 1_000);
        assert_eq!(arr[999], None);
    }

    #[test]
    fn test_uninit() {
        let _ = unsafe { super::new_uninit::<u64, 1_000>().unwrap() };
    }
}
