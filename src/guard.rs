use core::mem::{self, MaybeUninit};
use core::ptr;

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
        let initialized =
            ptr::slice_from_raw_parts_mut(self.array.as_mut_ptr().cast::<T>(), self.initialized);

        // SAFETY: The prefix contains initialized values with exclusive access;
        // the backing MaybeUninit array will not drop them again.
        unsafe { ptr::drop_in_place(initialized) };
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::Guard;
    use crate::box_ext::BoxExt;
    use alloc::rc::Rc;
    use core::cell::Cell;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use std::panic::{catch_unwind, AssertUnwindSafe};

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

    #[test]
    fn empty_prefix_cleanup() {
        struct MustNotDrop;

        impl Drop for MustNotDrop {
            fn drop(&mut self) {
                panic!("no elements were initialized");
            }
        }

        let result = crate::try_from_fn::<MustNotDrop, _, 5, _>(|_| Err("initialization failed"));

        assert!(matches!(result, Err("initialization failed")));
    }

    #[test]
    fn partial_drop_cleanup_zst() {
        struct DropCounter;

        static DROPS: AtomicUsize = AtomicUsize::new(0);

        impl Drop for DropCounter {
            fn drop(&mut self) {
                DROPS.fetch_add(1, Ordering::SeqCst);
            }
        }

        assert_eq!(core::mem::size_of::<DropCounter>(), 0);
        let result = crate::try_from_fn::<_, (), 5, _>(|index| match index {
            3 => Err(()),
            _ => Ok(DropCounter),
        });

        assert!(result.is_err());
        assert_eq!(DROPS.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn partial_drop_cleanup_after_initializer_panics() {
        struct DropCounter(Rc<Cell<usize>>);

        impl Drop for DropCounter {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }

        let drops = Rc::new(Cell::new(0));
        let result = catch_unwind(AssertUnwindSafe(|| {
            let _ = crate::from_fn::<_, 5, _>(|index| {
                assert!(index != 3, "intentional initializer panic");
                DropCounter(Rc::clone(&drops))
            });
        }));

        let panic = result.expect_err("initializer should panic");
        assert_eq!(
            panic.downcast_ref::<&str>(),
            Some(&"intentional initializer panic")
        );
        assert_eq!(drops.get(), 3);
        assert_eq!(Rc::strong_count(&drops), 1);
    }

    #[test]
    fn partial_drop_cleanup_after_destructor_panics() {
        struct DropTracker(Rc<Cell<usize>>);

        impl Drop for DropTracker {
            fn drop(&mut self) {
                let count = self.0.get() + 1;
                self.0.set(count);
                assert!(count != 1, "intentional destructor panic");
            }
        }

        let drops = Rc::new(Cell::new(0));
        let result = catch_unwind(AssertUnwindSafe(|| {
            let _ = crate::try_from_fn::<_, (), 5, _>(|index| {
                if index == 3 {
                    return Err(());
                }

                Ok(DropTracker(Rc::clone(&drops)))
            });
        }));

        let panic = result.expect_err("first destructor should panic");
        assert_eq!(
            panic.downcast_ref::<&str>(),
            Some(&"intentional destructor panic")
        );
        assert_eq!(drops.get(), 3);
        assert_eq!(Rc::strong_count(&drops), 1);
    }
}
