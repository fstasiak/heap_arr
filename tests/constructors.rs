#![warn(clippy::pedantic)]

use core::cell::Cell;
use core::sync::atomic::{AtomicUsize, Ordering};
use std::boxed::Box;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;
use std::string::String;

#[test]
fn new_copied() {
    const LEN: usize = 1024;

    let arr = heap_arr::new_copied::<Option<bool>, LEN>(&Some(true));
    assert_eq!(arr.len(), LEN);
    assert!(arr.iter().all(|value| *value == Some(true)));
}

#[test]
fn new_copied_does_not_clone() {
    #[derive(Copy)]
    struct CopyOnly(u8);

    // Deliberately differs from Copy to detect accidental cloning.
    #[allow(clippy::non_canonical_clone_impl, clippy::expl_impl_clone_on_copy)]
    impl Clone for CopyOnly {
        fn clone(&self) -> Self {
            panic!("new_copied must not call T::clone");
        }
    }

    let empty = heap_arr::new_copied::<_, 0>(&CopyOnly(7));
    assert!(empty.is_empty());
    let array = heap_arr::new_copied::<_, 4>(&CopyOnly(7));
    assert!(array.iter().all(|value| value.0 == 7));
}

#[test]
fn new_cloned() {
    const LEN: usize = 16;

    let arr = heap_arr::new_cloned::<String, LEN>(&String::from("value"));
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
            let _ = heap_arr::new_cloned::<_, LEN>(&initial);
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
        match catch_unwind(heap_arr::new_default::<DefaultTracker, LEN>) {
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

    check(heap_arr::new_cloned);
    check(|initial| heap_arr::from_fn(|_| Rc::clone(initial)));
    check(|initial| heap_arr::try_from_fn::<_, (), 4, _>(|_| Ok(Rc::clone(initial))).unwrap());
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

    assert!(heap_arr::new_copied::<u8, 0>(&7).is_empty());
    assert!(heap_arr::new_cloned::<_, 0>(&MustNotInitialize).is_empty());
    assert!(heap_arr::new_default::<MustNotInitialize, 0>().is_empty());
    assert!(heap_arr::from_fn::<String, 0, _>(|_| panic!("initializer must not run")).is_empty());
    assert!(
        heap_arr::try_from_fn::<String, (), 0, _>(|_| panic!("initializer must not run"))
            .unwrap()
            .is_empty()
    );
    assert!(heap_arr::uninit::<String, 0>().is_empty());
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

    check(&heap_arr::new_copied(&Aligned(7)), 7);
    check(&heap_arr::new_cloned(&Aligned(7)), 7);
    check(&heap_arr::new_default(), 0);
    check(&heap_arr::from_fn(|_| Aligned(7)), 7);
    check(
        &heap_arr::try_from_fn::<_, (), 3, _>(|_| Ok(Aligned(7))).unwrap(),
        7,
    );
    let array = heap_arr::uninit::<Aligned, 3>();
    assert!(array.as_ptr().is_aligned());
}

#[test]
fn default() {
    let arr = heap_arr::new_default::<Option<bool>, 1000>();
    assert_eq!(arr.len(), 1000);
    assert_eq!(arr[999], None);
}

#[test]
fn from_fn_initializes_in_order() {
    let mut calls = 0;
    let array = heap_arr::from_fn::<_, 4, _>(|index| {
        assert_eq!(index, calls);
        calls += 1;
        index * index
    });

    assert_eq!(calls, 4);
    assert_eq!(*array, [0, 1, 4, 9]);
}

#[test]
fn try_from_fn_initializes_in_order() {
    let mut calls = 0;
    let array = heap_arr::try_from_fn::<_, (), 4, _>(|index| {
        assert_eq!(index, calls);
        calls += 1;
        Ok(index * index)
    })
    .unwrap();

    assert_eq!(calls, 4);
    assert_eq!(*array, [0, 1, 4, 9]);
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
        let result = heap_arr::try_from_fn::<_, _, LEN, _>(|index| {
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
    let arr = heap_arr::uninit::<u64, LEN>();
    assert_eq!(arr.len(), LEN);
    core::hint::black_box(arr);
}
