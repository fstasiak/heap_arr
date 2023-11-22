# heap_arr
This is a small `#![no_std]` crate allowing for construction of arrays directly
on the heap.

Provided functions allow for creation of both initialized and uninitialized
arrays.

### rationale
Currently when someone tries to do `Box::new([0_u8; 1_000_000_000])` Rust will
create the array on stack and only then move it to the heap. For large arrays
this approach will obviously overflow the stack. This is optimized away and the
array is created directly on the heap when compiled in `release` mode but
sometimes, for example for testing purposes that require quick recompilations
in `debug` mode this is not an option.