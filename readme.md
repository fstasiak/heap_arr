# Heap Array (heap_arr)

The Heap Array crate is a lightweight `#![no_std]` library designed to facilitate
the construction of arrays directly on the heap.

The provided functions within this crate enable the creation of both initialized
and uninitialized arrays.

## Rationale

In certain scenarios, such as attempting to create a large array using
`Box::new([0_u8; 1_000_000_000])`, Rust defaults to creating the array on the
stack before moving it to the heap. This method poses a challenge for large
arrays as it can lead to stack overflow. While Rust optimizes this process in
`release` mode by directly allocating the array on the heap, there are
situations, such as frequent recompilations during testing in `debug` mode,
where this optimization is not viable.

The Heap Array crate addresses these challenges by providing a mechanism to
create arrays directly on the heap, offering a more flexible solution for
scenarios where stack limitations or debugging requirements come into play.