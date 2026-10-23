// Miri does not enforce native stack limits.
#![cfg(not(miri))]
#![warn(clippy::pedantic)]

use core::hint::black_box;
use std::thread;

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
        let arr = black_box(heap_arr::new_copied::<u8, LEN>(&7));
        assert_eq!(arr[LEN - 1], 7);
    });
}

#[test]
fn new_cloned() {
    run("new_cloned", || {
        let arr = black_box(heap_arr::new_cloned::<u8, LEN>(&7));
        assert_eq!(arr[LEN - 1], 7);
    });
}

#[test]
fn new_default() {
    run("new_default", || {
        let arr = black_box(heap_arr::new_default::<u8, LEN>());
        assert_eq!(arr[LEN - 1], 0);
    });
}

#[test]
fn from_fn() {
    run("from_fn", || {
        let arr = black_box(heap_arr::from_fn::<u8, LEN, _>(|_| 7));
        assert_eq!(arr[LEN - 1], 7);
    });
}

#[test]
fn try_from_fn() {
    run("try_from_fn", || {
        let arr = black_box(heap_arr::try_from_fn::<u8, (), LEN, _>(|_| Ok(7)).unwrap());
        assert_eq!(arr[LEN - 1], 7);
    });
}

#[test]
fn uninit() {
    run("uninit", || {
        let arr = black_box(heap_arr::uninit::<u8, LEN>());
        assert_eq!(arr.len(), LEN);
    });
}
