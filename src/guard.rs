use core::mem::{self, MaybeUninit};

extern crate alloc;

pub struct Guard<'a, T, const N: usize> {
    array: &'a mut [MaybeUninit<T>; N],
    initialized: usize,
}

impl<'a, T, const N: usize> Guard<'a, T, N> {
    pub fn new(array: &'a mut [MaybeUninit<T>; N]) -> Self {
        Self {
            array,
            initialized: 0,
        }
    }

    /// # Safety
    /// The caller must ensure not to call this method more than `N` times
    pub unsafe fn push_unchecked(&mut self, value: T) {
        debug_assert!(self.initialized < N);

        unsafe { self.array.get_unchecked_mut(self.initialized).write(value) };
        self.initialized += 1;
    }

    /// # Safety
    /// The caller must ensure that `self.initialized == N`, so that
    /// the [`Self::push_unchecked`] method has been called exactly `N` times
    pub unsafe fn finalize(self) {
        debug_assert_eq!(self.initialized, N);

        mem::forget(self);
    }
}

impl<T, const N: usize> Drop for Guard<'_, T, N> {
    fn drop(&mut self) {
        // Safety: We are dropping all initialized entries
        for i in 0..self.initialized {
            unsafe { self.array.get_unchecked_mut(i).assume_init_drop() };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Guard;
    use crate::box_ext::BoxExt;
    use core::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn init_tracking() {
        let mut array = crate::uninit::<u32, 5>();
        let mut guard = Guard::new(&mut array);

        assert_eq!(0, guard.initialized);
        unsafe { guard.push_unchecked(10) };
        assert_eq!(1, guard.initialized);
    }

    #[test]
    fn full_init() {
        let mut array = crate::uninit::<u32, 3>();
        let mut guard = Guard::new(&mut array);

        unsafe {
            guard.push_unchecked(1);
            guard.push_unchecked(2);
            guard.push_unchecked(3);
            guard.finalize();
        }

        assert_eq!(*unsafe { array.assume_init() }, [1, 2, 3]);
    }

    #[test]
    fn partial_drop_cleanup() {
        struct DropCounter {
            counter: &'static AtomicUsize,
        }

        impl Drop for DropCounter {
            fn drop(&mut self) {
                self.counter.fetch_add(1, Ordering::SeqCst);
            }
        }

        static DROPS: AtomicUsize = AtomicUsize::new(0);
        {
            let mut array = crate::uninit::<DropCounter, 5>();
            let mut guard = Guard::new(&mut array);

            unsafe {
                guard.push_unchecked(DropCounter { counter: &DROPS });
                guard.push_unchecked(DropCounter { counter: &DROPS });
            }
            // Guard drops here, should clean up 2 elements
        }

        assert_eq!(DROPS.load(Ordering::SeqCst), 2);
    }
}
