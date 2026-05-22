use crate::HEAP_SIZE;
use crate::linked_list::LinkedListAllocator;
use crate::locked::Locked;
use core::alloc::GlobalAlloc;
use std::alloc::Layout;

struct ListNode {
    next: Option<&'static mut ListNode>,
}

/// The block sizes to use.
///
/// The sizes must each be power of 2 because they are also used as
/// the block alignment (alignments must be always powers of 2).
const BLOCK_SIZES: &[usize] = &[8, 16, 32, 64, 128, 256, 512, 1024, 2048];

pub struct FixedSizeBlockAllocator {
    list_heads: [Option<&'static mut ListNode>; BLOCK_SIZES.len()],
    fallback_allocator: LinkedListAllocator,
}

impl Default for FixedSizeBlockAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl FixedSizeBlockAllocator {
    pub const fn new() -> Self {
        const EMPTY: Option<&'static mut ListNode> = None;
        FixedSizeBlockAllocator {
            list_heads: [EMPTY; BLOCK_SIZES.len()],
            fallback_allocator: LinkedListAllocator::new(),
        }
    }

    /// Initialize the allocator with the given heap bounds.
    /// # Safety
    ///
    /// The caller must ensure this is called only once.
    /// The mmap'd region must be unused.
    pub unsafe fn init(&mut self) {
        unsafe {
            self.fallback_allocator.init();
        }
    }

    /// Allocates using the fallback allocator
    fn fallback_alloc(&mut self, layout: Layout) -> *mut u8 {
        self.fallback_allocator.alloc(layout)
    }
}

/// Choose an appropriate block size for the given layout.
///
/// Returns an index into the `BLOCK_SIZES` array.
fn list_index(layout: &Layout) -> Option<usize> {
    let required_block_size = layout.size().max(layout.align());
    BLOCK_SIZES.iter().position(|&s| s >= required_block_size)
}

unsafe impl GlobalAlloc for Locked<FixedSizeBlockAllocator> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut allocator = self.lock();
        match list_index(&layout) {
            Some(index) => {
                match allocator.list_heads[index].take() {
                    Some(node) => {
                        allocator.list_heads[index] = node.next.take();
                        node as *mut ListNode as *mut u8
                    }
                    None => {
                        // no block exists in list, allocate a new one
                        let block_size = BLOCK_SIZES[index];
                        // only works if all block sizes are a power of 2
                        let block_align = block_size;
                        let layout = Layout::from_size_align(block_size, block_align).unwrap();
                        allocator.fallback_alloc(layout)
                    }
                }
            }
            None => allocator.fallback_alloc(layout),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let mut allocator = self.lock();
        match list_index(&layout) {
            Some(index) => {
                let new_node = ListNode {
                    next: allocator.list_heads[index].take(),
                };
                // verify that block has size and alignment required for storing node
                assert!(std::mem::size_of::<ListNode>() <= BLOCK_SIZES[index]);
                assert!(std::mem::align_of::<ListNode>() <= BLOCK_SIZES[index]);
                let new_node_ptr = ptr as *mut ListNode;
                unsafe {
                    new_node_ptr.write(new_node);
                    allocator.list_heads[index] = Some(&mut *new_node_ptr);
                }
            }
            None => allocator.fallback_allocator.dealloc(ptr, layout),
        }
    }
}

impl Locked<FixedSizeBlockAllocator> {
    pub fn bytes_allocated(&self) -> usize {
        let guard = self.lock();
        let mut fixed_free = 0;
        for (index, head) in guard.list_heads.iter().enumerate() {
            let mut current = head;
            while let Some(node) = current {
                fixed_free += BLOCK_SIZES[index];
                current = &node.next;
            }
        }

        let fallback_free = guard.fallback_allocator.free_bytes();

        HEAP_SIZE - (fixed_free + fallback_free)
    }
}
