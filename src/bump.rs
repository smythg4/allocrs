use libc::{MAP_ANONYMOUS, MAP_FAILED, MAP_PRIVATE, PROT_READ, PROT_WRITE};
use std::alloc::{GlobalAlloc, Layout};
use std::os::raw::c_void;
use crate::locked::Locked;
use crate::HEAP_SIZE;

impl Locked<BumpAllocator> {
    pub fn bytes_allocated(&self) -> usize {
        let bump = self.lock();
        bump.next - bump.heap_start
    }
}

pub struct BumpAllocator {
    heap_start: usize,
    heap_end: usize,
    next: usize,
    allocations: usize,
}

impl Default for BumpAllocator {
    fn default() -> Self {
        Self::new()
    }
}
impl BumpAllocator {
    /// Creates a new empty bump allocator.
    pub const fn new() -> Self {
        BumpAllocator {
            heap_start: 0,
            heap_end: 0,
            next: 0,
            allocations: 0,
        }
    }

    /// Initializes the bump allocator with the given heap bounds.
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

        self.heap_start = ptr as usize;
        self.heap_end = self.heap_start + HEAP_SIZE;
        self.next = self.heap_start;
    }
}

unsafe impl GlobalAlloc for Locked<BumpAllocator> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut bump = self.lock();

        if bump.heap_start == 0 {
            unsafe { bump.init() };
        }

        let alloc_start = align_up(bump.next, layout.align());
        let alloc_end = match alloc_start.checked_add(layout.size()) {
            Some(end) => end,
            None => return std::ptr::null_mut(),
        };

        if alloc_end > bump.heap_end {
            std::ptr::null_mut() // Out of memory
        } else {
            bump.next = alloc_end;
            bump.allocations += 1;
            alloc_start as *mut u8
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        let mut bump = self.lock();
        bump.allocations -= 1;
        if bump.allocations == 0 {
            bump.next = bump.heap_start;
        }
    }
}

/// Align the given address `addr` upwards to alignment `align`.
fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}
