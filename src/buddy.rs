use crate::HEAP_SIZE;
use crate::locked::Locked;
use libc::{MAP_ANONYMOUS, MAP_FAILED, MAP_PRIVATE, PROT_READ, PROT_WRITE};
use std::alloc::{GlobalAlloc, Layout};
use std::os::raw::c_void;

// pub const ORDERS: usize = 14; // log_2(HEAP_SIZE=1MB / MIN_BLOCK_SIZE=64B)
// pub const MIN_BLOCK_SIZE: usize = 64;

struct ListNode {
    next: Option<&'static mut ListNode>,
}

pub struct BuddyAllocator<const MIN_BLOCK_SIZE: usize, const ORDERS: usize> {
    free_lists: [Option<&'static mut ListNode>; ORDERS],
    heap_start: usize,
    initialized: bool,
}

impl<const MIN_BLOCK_SIZE: usize, const ORDERS: usize> Default
    for BuddyAllocator<MIN_BLOCK_SIZE, ORDERS>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<const MIN_BLOCK_SIZE: usize, const ORDERS: usize> BuddyAllocator<MIN_BLOCK_SIZE, ORDERS> {
    pub const fn new() -> Self {
        const EMPTY: Option<&'static mut ListNode> = None;
        BuddyAllocator {
            free_lists: [EMPTY; ORDERS],
            heap_start: 0,
            initialized: false,
        }
    }

    /// Initializes the buddy allocator with the pre-defined heap size.
    /// # Safety
    ///
    /// The caller must ensure this is called only once.
    /// The mmap'd region must be unused.
    pub unsafe fn init(&mut self) {
        assert_eq!(
            MIN_BLOCK_SIZE << ORDERS,
            HEAP_SIZE,
            "ORDERS must equal log2(HEAP_SIZE / MIN_BLOCK_SIZE)"
        );
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                HEAP_SIZE,
                PROT_READ | PROT_WRITE,
                MAP_ANONYMOUS | MAP_PRIVATE,
                -1,
                0,
            )
        };
        if ptr == MAP_FAILED {
            let msg = b"Failed to build memory mapped area. Exiting...";
            unsafe { libc::write(2, msg.as_ptr() as *const c_void, msg.len()) };
            std::process::exit(1);
        }
        self.heap_start = ptr as usize;
        self.add_free_block(self.heap_start, ORDERS - 1);
        self.add_free_block(self.heap_start + HEAP_SIZE / 2, ORDERS - 1);
        self.initialized = true;
    }

    fn add_free_block(&mut self, addr: usize, order: usize) {
        // ensure freed region is capable of holding ListNode
        assert_eq!(align_up(addr, std::mem::size_of::<ListNode>()), addr);
        let size = MIN_BLOCK_SIZE << order;
        assert!(size >= std::mem::size_of::<ListNode>());

        let next = self.free_lists[order].take();
        let new_node = ListNode { next };
        let node_ptr = addr as *mut ListNode;
        unsafe {
            node_ptr.write(new_node);
            self.free_lists[order] = Some(&mut *node_ptr)
        };
    }

    fn split_block(&mut self, order: usize) -> Option<usize> {
        if order >= ORDERS {
            return None;
        }

        if let Some(block) = self.free_lists[order].take() {
            self.free_lists[order] = block.next.take();
            return Some(block as *mut ListNode as usize);
        }

        // no block at this order — split one from above
        let addr = self.split_block(order + 1)?;
        let buddy = addr + (MIN_BLOCK_SIZE << order);
        self.add_free_block(buddy, order);
        Some(addr)
    }

    pub fn alloc(&mut self, layout: Layout) -> *mut u8 {
        if !self.initialized {
            unsafe { self.init() };
        }
        match self.orders_index(&layout) {
            Some(order) => match self.free_lists[order].take() {
                Some(node) => {
                    self.free_lists[order] = node.next.take();
                    node as *mut ListNode as *mut u8
                }
                None => self
                    .split_block(order)
                    .map(|i| i as *mut u8)
                    .unwrap_or(std::ptr::null_mut()),
            },
            None => std::ptr::null_mut(),
        }
    }

    pub fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        let this_addr = ptr as usize;
        let order = self.orders_index(&layout).unwrap();
        if order == ORDERS - 1 {
            self.add_free_block(this_addr, order);
            return;
        }
        let buddy = self.heap_start + ((this_addr - self.heap_start) ^ (MIN_BLOCK_SIZE << order));

        if Self::remove_buddy(&mut self.free_lists[order], buddy) {
            let merged = this_addr.min(buddy);
            let next_layout = Layout::from_size_align(
                MIN_BLOCK_SIZE << (order + 1),
                MIN_BLOCK_SIZE << (order + 1),
            )
            .unwrap();
            self.dealloc(merged as *mut u8, next_layout);
        } else {
            self.add_free_block(this_addr, order);
        }
    }

    fn remove_buddy(list: &mut Option<&'static mut ListNode>, buddy: usize) -> bool {
        match list {
            None => false,
            Some(node) => {
                if *node as *mut ListNode as usize == buddy {
                    *list = node.next.take();
                    true
                } else {
                    Self::remove_buddy(&mut node.next, buddy)
                }
            }
        }
    }

    /// Choose an appropriate order for the given layout.
    ///
    /// Returns an index into the `free_list` array.
    fn orders_index(&self, layout: &Layout) -> Option<usize> {
        let required_block_size = layout.size().max(layout.align()).max(MIN_BLOCK_SIZE);
        if required_block_size > MIN_BLOCK_SIZE << (ORDERS - 1) {
            return None;
        }
        Some(
            required_block_size.next_power_of_two().trailing_zeros() as usize
                - MIN_BLOCK_SIZE.trailing_zeros() as usize,
        )
    }
}

/// Align the given address `addr` upwards to alignment `align`.
fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

unsafe impl<const MIN_BLOCK_SIZE: usize, const ORDERS: usize> GlobalAlloc
    for Locked<BuddyAllocator<MIN_BLOCK_SIZE, ORDERS>>
{
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut allocator = self.lock();
        allocator.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let mut allocator = self.lock();
        allocator.dealloc(ptr, layout)
    }
}

impl<const MIN_BLOCK_SIZE: usize, const ORDERS: usize>
    Locked<BuddyAllocator<MIN_BLOCK_SIZE, ORDERS>>
{
    pub fn bytes_allocated(&self) -> usize {
        let allocator = self.lock();
        let mut free_bytes = 0;
        for (order, list) in allocator.free_lists.iter().enumerate() {
            let mut current = list;
            while let Some(node) = current {
                free_bytes += MIN_BLOCK_SIZE << order;
                current = &node.next;
            }
        }
        HEAP_SIZE - free_bytes
    }
}
