use alloc::boxed::Box;
use core::mem::{ManuallyDrop, MaybeUninit};
use core::ptr;

use crate::BoxExt;

pub struct Guard<T, const N: usize> {
    array: Box<[MaybeUninit<T>; N]>,
    initialized: usize,
}

impl<T, const N: usize> Guard<T, N> {
    pub fn new() -> Self {
        Self {
            array: crate::uninit(),
            initialized: 0,
        }
    }

    pub fn push(&mut self, value: T) {
        self.array[self.initialized].write(value);
        self.initialized += 1;
    }

    pub fn finish(self) -> Box<[T; N]> {
        assert_eq!(self.initialized, N);
        let guard = ManuallyDrop::new(self);

        // SAFETY: All entries are initialized. ManuallyDrop prevents cleanup,
        // so reading the Box transfers sole ownership to the initialized result.
        unsafe { ptr::read(&raw const guard.array).assume_init() }
    }
}

impl<T, const N: usize> Drop for Guard<T, N> {
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
    use alloc::rc::Rc;
    use core::cell::Cell;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use std::panic::{catch_unwind, AssertUnwindSafe};

    #[test]
    fn init_tracking() {
        let mut guard = Guard::<u32, 5>::new();

        assert_eq!(0, guard.initialized);
        guard.push(10);
        assert_eq!(1, guard.initialized);
    }

    #[test]
    fn full_init() {
        let mut guard = Guard::<u32, 3>::new();
        guard.push(1);
        guard.push(2);
        guard.push(3);

        assert_eq!(*guard.finish(), [1, 2, 3]);
    }

    #[test]
    fn incomplete_finish_cleans_up() {
        let resource = Rc::new(());
        let result = catch_unwind(|| {
            let mut guard = Guard::<Rc<()>, 3>::new();
            guard.push(Rc::clone(&resource));
            guard.push(Rc::clone(&resource));
            let _ = guard.finish();
        });

        assert!(result.is_err());
        assert_eq!(Rc::strong_count(&resource), 1);
    }

    #[test]
    fn push_past_capacity_preserves_initialized_values() {
        let resource = Rc::new(());
        let mut guard = Guard::<Rc<()>, 1>::new();
        guard.push(Rc::clone(&resource));

        let result = catch_unwind(AssertUnwindSafe(|| {
            guard.push(Rc::clone(&resource));
        }));

        assert!(result.is_err());
        assert_eq!(Rc::strong_count(&resource), 2);
        drop(guard.finish());
        assert_eq!(Rc::strong_count(&resource), 1);
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
            let mut guard = Guard::<DropCounter, 5>::new();
            guard.push(DropCounter { counter: &DROPS });
            guard.push(DropCounter { counter: &DROPS });
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
