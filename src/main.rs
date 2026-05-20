use allocrs::bump_allocator::{Locked, BumpAllocator};

/// Set the global allocator.
#[global_allocator]
pub static GLOBAL_ALLOCATOR: Locked<BumpAllocator> = Locked::new(BumpAllocator::new());

fn main() {
    let v: Vec<usize> = (0..100).collect(); // Allocates from the bump allocator
    println!("{:?}", v);
    let total_memory_allocated = GLOBAL_ALLOCATOR.bytes_allocated();
    println!("Total memory allocated: {} bytes", total_memory_allocated);
    drop(v);
    let v2 = vec![1, 2, 3, 4, 5];
    println!("{:?}", v2);
    let total_memory_allocated = GLOBAL_ALLOCATOR.bytes_allocated();
    println!("Total memory allocated: {} bytes", total_memory_allocated);
}
