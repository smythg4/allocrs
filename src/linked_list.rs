use crate::HEAP_SIZE;
use crate::locked::Locked;
use libc::{MAP_ANONYMOUS, MAP_FAILED, MAP_PRIVATE, PROT_READ, PROT_WRITE};
use std::alloc::{GlobalAlloc, Layout};
use std::os::raw::c_void;
use std::ptr;

struct ListNode {
    size: usize,
    next: Option<&'static mut ListNode>,
}

impl ListNode {
    const fn new(size: usize) -> Self {
        ListNode { size, next: None }
    }

    fn start_addr(&self) -> usize {
        self as *const Self as usize
    }

    fn end_addr(&self) -> usize {
        self.start_addr() + self.size
    }
}

pub struct LinkedListAllocator {
    head: ListNode,
    initialized: bool,
}

impl Default for LinkedListAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl LinkedListAllocator {
    /// Creates an empty LinkedListAllocator
    pub const fn new() -> Self {
        Self {
            head: ListNode::new(0),
            initialized: false,
        }
    }

    /// Initializes the linked list allocator with the pre-defined heap size.
    /// # Safety
    ///
    /// The caller must ensure this is called only once.
    /// The mmap'd region must be unused.
    pub unsafe fn init(&mut self) {
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
        unsafe { self.add_free_region(ptr as usize, HEAP_SIZE) }
        self.initialized = true;
    }

    /// Adds the given memory region to the front of the list.
    unsafe fn add_free_region(&mut self, addr: usize, size: usize) {
        // ensure freed region is capable of holding ListNode
        assert_eq!(align_up(addr, std::mem::size_of::<ListNode>()), addr);
        assert!(size >= std::mem::size_of::<ListNode>());

        // keep list sorted by start_addr()
        let mut current = &mut self.head;
        while let Some(ref mut region) = current.next
            && region.start_addr() < addr
        {
            current = current.next.as_mut().unwrap()
        }
        // create a new list node and append it at the start of the list
        let mut node = ListNode::new(size);
        node.next = current.next.take();

        let node_ptr = addr as *mut ListNode;
        unsafe {
            node_ptr.write(node);
            current.next = Some(&mut *node_ptr)
        }

        // coalesce the free regions
        if let Some(new_node) = current.next.as_mut() {
            let new_node_end = new_node.end_addr();
            if let Some(ref mut successor) = new_node.next
                && new_node_end == successor.start_addr()
            {
                new_node.size += successor.size;
                new_node.next = successor.next.take();
            }
        }
        if current.end_addr() == addr {
            current.size += current.next.as_ref().map_or(0, |n| n.size);
            current.next = current.next.as_mut().and_then(|n| n.next.take());
        }
    }

    /// Looks for a free region with the given size and alignment and removes
    /// it from the list.
    ///
    /// Returns a tuple of the list node and the start address of the allocation.
    fn find_region(&mut self, size: usize, align: usize) -> Option<(&'static mut ListNode, usize)> {
        // reference to current list node, updated for each iteration
        let mut current = &mut self.head;
        // look for a large enough memory region in the linked list
        while let Some(ref mut region) = current.next {
            if let Ok(alloc_start) = Self::alloc_from_region(region, size, align) {
                // region suitable for allocation -> remove node from list
                let next = region.next.take();
                let ret = Some((current.next.take().unwrap(), alloc_start));
                current.next = next;
                return ret;
            } else {
                // region not suitable -> continue with next region
                current = current.next.as_mut().unwrap();
            }
        }

        // no suitable region found
        None
    }

    /// Try to use the given region for an allocation with given size and
    /// alignment.
    ///
    /// Returns the allocation start address on success.
    fn alloc_from_region(region: &ListNode, size: usize, align: usize) -> Result<usize, ()> {
        let alloc_start = align_up(region.start_addr(), align);
        let alloc_end = alloc_start.checked_add(size).ok_or(())?;

        if alloc_end > region.end_addr() {
            // region too small
            return Err(());
        }

        let excess_size = region.end_addr() - alloc_end;
        if excess_size > 0 && excess_size < std::mem::size_of::<ListNode>() {
            // rest of region too small to hold a ListNode
            return Err(());
        }

        Ok(alloc_start)
    }

    /// Adjust the given layout so that the resulting allocated memory
    /// region is also capable of storing a `ListNode`.
    ///
    /// Returns the adjusted size and alignment as a (size, align) tuple.
    fn size_align(layout: Layout) -> (usize, usize) {
        let layout = layout
            .align_to(std::mem::size_of::<ListNode>())
            .expect("adjusting alignment failed")
            .pad_to_align();
        let size = layout.size().max(std::mem::size_of::<ListNode>());
        (size, layout.align())
    }

    pub fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let (size, align) = LinkedListAllocator::size_align(layout);
        if !self.initialized {
            unsafe { self.init() };
        }

        if let Some((region, alloc_start)) = self.find_region(size, align) {
            let alloc_end = alloc_start.checked_add(size).expect("overflow");
            let excess_size = region.end_addr() - alloc_end;
            if excess_size > 0 {
                unsafe {
                    self.add_free_region(alloc_end, excess_size);
                }
            }
            alloc_start as *mut u8
        } else {
            ptr::null_mut()
        }
    }

    pub fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        // perform layout adjustments
        let (size, _) = LinkedListAllocator::size_align(layout);

        unsafe { self.add_free_region(ptr as usize, size) }
    }

    pub fn free_bytes(&self) -> usize {
        let mut total_free = 0;
        let mut current = &self.head;
        while let Some(node) = &current.next {
            total_free += node.size;
            current = node;
        }
        total_free
    }
}

impl Locked<LinkedListAllocator> {
    pub fn bytes_allocated(&self) -> usize {
        let total_free = self.lock().free_bytes();
        HEAP_SIZE - total_free
    }
}

unsafe impl GlobalAlloc for Locked<LinkedListAllocator> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut allocator = self.lock();
        allocator.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let mut allocator = self.lock();
        allocator.dealloc(ptr, layout);
    }
}

/// Align the given address `addr` upwards to alignment `align`.
fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}
