use allocrs::fixed_size_block::FixedSizeBlockAllocator;
use allocrs::locked::Locked;

/// Set the global allocator.
#[global_allocator]
pub static GLOBAL_ALLOCATOR: Locked<FixedSizeBlockAllocator> =
    Locked::new(FixedSizeBlockAllocator::new());

/// Pre-defined heap size
const HEAP_SIZE: usize = 1024 * 1024; // 1 MB heap

#[test]
fn simple_allocation() {
    let x = Box::new(42);
    assert_eq!(*x, 42);
}

#[test]
fn large_vec() {
    let n = 1000;
    let mut v = Vec::with_capacity(n);
    for i in 0..n {
        v.push(i);
    }
    assert_eq!(v.len(), n);
}

#[test]
fn many_boxes() {
    for i in 0..1000 {
        let x = Box::new(i);
        assert_eq!(*x, i);
    }
}

#[test]
fn many_boxes_long_lived() {
    let long_lived = Box::new(1);
    for i in 0..HEAP_SIZE {
        let x = Box::new(i);
        assert_eq!(*x, i);
    }
    assert_eq!(*long_lived, 1);
}
