#![warn(clippy::pedantic)]
#![no_std]

use core::ptr;

use alloc::{
    alloc::{handle_alloc_error, Layout, LayoutError},
    boxed::Box,
};

extern crate alloc;

/// Allocates `[T; N]` on heap and initializes its entries to `default`.
///
/// # Safety
/// The result of this function is undefined if `mem::size_of::<T>() == 0`, or
/// if `N == 0`
///
/// # Errors
/// See [`Layout::array`]
pub unsafe fn new<T, const N: usize>(default: T) -> Result<Box<[T; N]>, LayoutError>
where
    T: Clone,
{
    unsafe {
        let ptr = new_uninit()?;
        for v in &mut *ptr {
            ptr::write(v, default.clone());
        }

        Ok(Box::from_raw(ptr))
    }
}

/// Allocates `[T; N]` on heap without initializing its entries.
/// The returned pointer is guaranteed to be non-null
///
/// # Safety
/// The result of this function is undefined if `mem::size_of::<T>() == 0`, or
/// if `N == 0`
///
/// # Errors
/// See [`Layout::array`]
pub unsafe fn new_uninit<T, const N: usize>() -> Result<*mut [T; N], LayoutError> {
    let layout = Layout::array::<T>(N)?;
    let ptr = alloc::alloc::alloc(layout);

    if ptr.is_null() {
        handle_alloc_error(layout);
    }

    Ok(ptr.cast())
}
