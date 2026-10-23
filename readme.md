# heap_arr

Allocate fixed-size arrays directly on the heap without constructing the entire
array on the stack. Works with `no_std` and `alloc`.

## The Problem

`Box::new([value; N])` can construct an intermediate array on the stack. For large
arrays, that can overflow the stack. Optimizations may eliminate the temporary,
but that behavior is not guaranteed.

## The Solution

```rust
let array: Box<[u8; 1_000_000]> = heap_arr::new_copied(&0);
assert_eq!(array[999_999], 0);
```

The crate allocates the array on the heap and initializes one element at a time.
The allocation must fit in available heap memory. Initializers, including
`Clone` and `Default` implementations, can still use stack space of their own.

## Choosing a constructor

| Function | Initialization | Return type |
| --- | --- | --- |
| `new_copied(&value)` | Copies a `Copy` value; does not call `T::clone` | `Box<[T; N]>` |
| `new_cloned(&value)` | Calls `value.clone()` for each element | `Box<[T; N]>` |
| `new_default()` | Calls `T::default()` for each element | `Box<[T; N]>` |
| `from_fn(f)` | Calls `f(index)` in ascending index order | `Box<[T; N]>` |
| `try_from_fn(f)` | Calls a fallible initializer in ascending index order | `Result<Box<[T; N]>, E>` |
| `uninit()` | Leaves every element uninitialized | `Box<[MaybeUninit<T>; N]>` |

For an empty array, initializers are not called. Use `BoxExt::assume_init` only
after every element of an `uninit()` allocation contains a valid `T`.

## Errors and cleanup

`try_from_fn` returns the first initializer error unchanged, stops initialization,
and drops the initialized elements. Allocation failures are handled separately
by the allocator and allocation error handler; they are not returned as `E`.

The safe constructors also drop initialized elements during panic unwinding.
Destructors are not guaranteed to run if the process aborts.

With `uninit()`, cleanup is the caller's responsibility: dropping the box does
not drop any `T` values already written into it. Prefer the safe constructors
when possible.

## `no_std`

The crate depends only on `alloc`. The final application must provide a global
allocator and any allocation error handling required by its target. Import
`Box` from `alloc::boxed` when using it in a `no_std` type annotation.

## Development

The minimum supported Rust version is 1.82.

```sh
cargo test
cargo test --release
cargo clippy --all-targets -- -D warnings
cargo +nightly miri test
```

Miri requires the component installed for the nightly toolchain. Native stack
limit tests are excluded under Miri; other unit tests, integration tests, and
documentation examples remain part of the checks. Large documentation examples
marked `no_run` are compiled without executing their allocations.
