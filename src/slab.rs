use crate::PAGE_SIZE;
use libc::{MAP_ANONYMOUS, MAP_FAILED, MAP_PRIVATE, PROT_READ, PROT_WRITE};
use std::marker::PhantomData;
use std::os::raw::c_void;

struct ListNode {
    next: Option<&'static mut ListNode>,
}

pub struct Slab<T> {
    ptr: *mut u8,
    free_list: Option<&'static mut ListNode>,
    used: usize,
    num_slots: usize,
    next: Option<Box<Slab<T>>>,
    _marker: PhantomData<T>,
}

impl<T> Default for Slab<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Slab<T> {
    pub fn new() -> Self {
        assert!(
            std::mem::size_of::<T>() < PAGE_SIZE,
            "Cannot allocate memory for types larger than OS page size"
        );
        // allocate a page to back the slab
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                PAGE_SIZE,
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
        let ptr = ptr as *mut u8;

        // initialize the free_list
        let slot_stride = std::mem::size_of::<T>().max(std::mem::size_of::<ListNode>());
        let num_slots = PAGE_SIZE / slot_stride;
        let mut next = None;
        for i in (0..num_slots).rev() {
            let node = unsafe { ptr.add(i * slot_stride) as *mut ListNode };
            unsafe { node.write(ListNode { next }) };
            next = Some(unsafe { &mut *node });
        }

        let free_list = next;
        Slab {
            ptr,
            free_list,
            used: 0,
            num_slots,
            next: None,
            _marker: PhantomData,
        }
    }

    pub fn is_full(&self) -> bool {
        self.used >= self.num_slots
    }

    pub fn is_empty(&self) -> bool {
        self.used == 0
    }

    pub fn num_slots(&self) -> usize {
        self.num_slots
    }

    pub fn start_addr(&self) -> *mut u8 {
        self.ptr
    }

    pub fn contains(&self, ptr: *mut T) -> bool {
        let ptr = ptr as usize;
        let start = self.ptr as usize;
        ptr >= start && ptr < start + PAGE_SIZE
    }

    pub fn alloc(&mut self) -> Option<*mut T> {
        self.free_list.take().map(|node| {
            self.free_list = node.next.take();
            self.used += 1;
            node as *mut ListNode as *mut T
        })
    }

    pub fn dealloc(&mut self, ptr: *mut T) {
        let new_node = ListNode {
            next: self.free_list.take(),
        };
        let new_node_ptr = ptr as *mut ListNode;
        unsafe {
            new_node_ptr.write(new_node);
            self.free_list = Some(&mut *new_node_ptr);
        }
        self.used -= 1;
    }
}

impl<T> Drop for Slab<T> {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.ptr as *mut c_void, PAGE_SIZE) };
        let mut cur = self.next.take();
        while let Some(mut slab) = cur {
            cur = slab.next.take();
            // slab drops here — calls munmap, finds next is None, returns
        }
    }
}

pub struct SlabCache<T> {
    pub full: Option<Box<Slab<T>>>,
    pub empty: Option<Box<Slab<T>>>,
    pub partial: Option<Box<Slab<T>>>,
}

impl<T> Default for SlabCache<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> SlabCache<T> {
    pub fn new() -> Self {
        SlabCache {
            full: None,
            empty: None,
            partial: None,
        }
    }

    pub fn alloc(&mut self) -> *mut T {
        let mut slab = self
            .partial
            .take()
            .or_else(|| self.empty.take())
            .unwrap_or_else(|| Box::new(Slab::new()));
        match slab.alloc() {
            Some(ptr) => {
                if slab.is_full() {
                    slab.next = self.full.take();
                    self.full = Some(slab);
                } else {
                    slab.next = self.partial.take();
                    self.partial = Some(slab);
                }
                ptr
            }
            None => unreachable!(),
        }
    }

    pub fn dealloc(&mut self, ptr: *mut T) {
        let mut slab = Self::remove_containing(&mut self.partial, ptr)
            .or_else(|| Self::remove_containing(&mut self.full, ptr))
            .or_else(|| Self::remove_containing(&mut self.empty, ptr))
            .expect("pointer not owned by this cache");

        slab.dealloc(ptr);
        if slab.is_empty() {
            slab.next = self.empty.take();
            self.empty = Some(slab);
        } else {
            slab.next = self.partial.take();
            self.partial = Some(slab);
        }
    }

    fn remove_containing(list: &mut Option<Box<Slab<T>>>, ptr: *mut T) -> Option<Box<Slab<T>>> {
        match list {
            None => None,
            Some(slab) if slab.contains(ptr) => list.take(),
            Some(slab) => Self::remove_containing(&mut slab.next, ptr),
        }
    }
}
