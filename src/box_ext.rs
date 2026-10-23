use core::mem::{self, MaybeUninit};

use alloc::boxed::Box;

/// Extension methods for boxed arrays of [`MaybeUninit`] elements.
pub trait BoxExt<T, const N: usize>: Sized {
    /// Converts `Box<[MaybeUninit<T>; N]>` into `Box<[T; N]>` without reallocating.
    ///
    /// # Safety
    /// The caller must ensure that all entries of the input array are
    /// initialized.
    ///
    /// # Examples
    /// ```
    /// use heap_arr::BoxExt;
    ///
    /// let mut array = heap_arr::uninit::<u8, 3>();
    /// for element in &mut *array {
    ///     element.write(7);
    /// }
    ///
    /// // SAFETY: Every element has been initialized.
    /// let array = unsafe { array.assume_init() };
    /// assert_eq!(*array, [7; 3]);
    /// ```
    #[must_use]
    unsafe fn assume_init(self) -> Box<[T; N]>;
}

impl<T, const N: usize> BoxExt<T, N> for Box<[MaybeUninit<T>; N]> {
    unsafe fn assume_init(self) -> Box<[T; N]> {
        // SAFETY: Both array types have identical layouts, and the caller guarantees
        // all elements are initialized. Consuming `self` transfers ownership of the
        // allocation to the resulting `Box`.
        unsafe { mem::transmute(self) }
    }
}
