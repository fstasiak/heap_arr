# heap_arr

Allocate fixed-size arrays directly on the heap. For `#![no_std]` environments too.

## The Problem

In Rust, `Box::new([T; N])` allocates on the stack first, then moves to the
heap. For large arrays, this can overflow the stack. Rust's optimizer should
handle this in optimized builds, but not in debug.

## The Solution

```rust
// This can overflow the stack in debug:
// let arr = Box::new([0_u8; 1_000_000_000]);

// Use this:
let arr: Box<[u8; 1_000_000_000]> = heap_arr::new_copied(&0_u8); // Allocates directly on heap
```

Fill arrays via `new_cloned()`, `new_default()`, `from_fn()`, `try_from_fn()`,
or grab uninitialized memory with `uninit()` and initialize it yourself.

Everything works in terms of `Box<[T; N]>` like the standard library.
No dependencies outside `alloc`, and works in `#![no_std]`.
