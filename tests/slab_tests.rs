use allocrs::PAGE_SIZE;
use allocrs::slab::{Slab, SlabCache};

// --- Slab tests ---

#[test]
fn slab_alloc_returns_nonnull() {
    let mut slab = Slab::<u64>::new();
    let ptr = slab.alloc();
    assert!(ptr.is_some());
    assert!(!ptr.unwrap().is_null());
}

#[test]
fn slab_fills_to_capacity() {
    let mut slab = Slab::<u64>::new();
    let num_slots = slab.num_slots();
    for _ in 0..num_slots {
        assert!(slab.alloc().is_some());
    }
    assert!(slab.is_full());
    assert!(slab.alloc().is_none());
}

#[test]
fn slab_dealloc_frees_slot() {
    let mut slab = Slab::<u64>::new();
    let ptr = slab.alloc().unwrap();
    slab.dealloc(ptr);
    assert!(slab.is_empty());
    // freed slot is reused on next alloc
    let ptr2 = slab.alloc().unwrap();
    assert_eq!(ptr, ptr2);
}

#[test]
fn slab_contains_inside() {
    let slab = Slab::<u64>::new();
    let first_slot = slab.start_addr() as *mut u64;
    let last_slot = (slab.start_addr() as usize + PAGE_SIZE - 8) as *mut u64;
    assert!(slab.contains(first_slot));
    assert!(slab.contains(last_slot));
}

#[test]
fn slab_contains_outside() {
    let slab = Slab::<u64>::new();
    let just_past_end = (slab.start_addr() as usize + PAGE_SIZE) as *mut u64;
    assert!(!slab.contains(std::ptr::null_mut()));
    assert!(!slab.contains(just_past_end));
}

// --- SlabCache tests ---
// Uses [u8; 2048] so num_slots = PAGE_SIZE / 2048 = 2, making routing easy to trigger.

#[test]
fn cache_basic_alloc_dealloc() {
    let mut cache = SlabCache::<u64>::new();
    let ptr = cache.alloc();
    assert!(!ptr.is_null());
    cache.dealloc(ptr);
}

#[test]
fn cache_full_slab_moves_to_full_list() {
    let mut cache = SlabCache::<[u8; 2048]>::new();
    let a = cache.alloc();
    let b = cache.alloc(); // slab should now be full
    assert!(cache.full.is_some());
    assert!(cache.partial.is_none());
    cache.dealloc(a);
    cache.dealloc(b);
}

#[test]
fn cache_dealloc_full_to_partial() {
    let mut cache = SlabCache::<[u8; 2048]>::new();
    let a = cache.alloc();
    let b = cache.alloc();
    assert!(cache.full.is_some());
    cache.dealloc(a); // slab goes from full → partial
    assert!(cache.partial.is_some());
    cache.dealloc(b);
}

#[test]
fn cache_dealloc_partial_to_empty() {
    let mut cache = SlabCache::<[u8; 2048]>::new();
    let a = cache.alloc();
    let b = cache.alloc();
    cache.dealloc(a);
    cache.dealloc(b); // slab goes from partial → empty
    assert!(cache.empty.is_some());
    assert!(cache.partial.is_none());
}

#[test]
fn cache_reuses_empty_slab_before_creating_new() {
    let mut cache = SlabCache::<[u8; 2048]>::new();
    let a = cache.alloc();
    let b = cache.alloc();
    cache.dealloc(a);
    cache.dealloc(b);
    assert!(cache.empty.is_some());
    // next alloc should pull from empty, not mmap a new slab
    let _ = cache.alloc();
    assert!(cache.empty.is_none());
}

#[test]
fn cache_spans_multiple_slabs() {
    let mut cache = SlabCache::<[u8; 2048]>::new();
    // 4 allocs with num_slots=2 forces a second slab
    let ptrs: Vec<_> = (0..4).map(|_| cache.alloc()).collect();
    assert!(cache.full.is_some());
    for ptr in ptrs {
        cache.dealloc(ptr);
    }
    assert!(cache.empty.is_some());
}
