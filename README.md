# allocrs
  Building custom memory allocators in Rust, following Phil Opp's [_Writing an OS in
  Rust_](https://os.phil-opp.com/allocator-designs/) blog — but adapted for macOS userspace
  instead of bare metal x86_64.

  ## Design

  All allocators are backed by anonymous `mmap` regions rather than a static buffer or a fixed
  compile-time heap. Memory is acquired from the OS on first allocation and managed entirely by
   the allocator from there.

  Thread safety is handled by a generic `Locked<A>` wrapper — a spinlock using `AtomicBool` and
   `UnsafeCell<A>` — rather than `std::sync::Mutex`, which cannot be used inside a
  `GlobalAlloc` implementation without risking recursive allocation.

  ## Implementations

  ### Bump Allocator
  Allocates by advancing a pointer through the heap. No individual deallocation — memory is
  only reclaimed when every live allocation has been freed, at which point the pointer resets
  to the heap start. Fast and simple, but limited to workloads where all allocations share the
  same lifetime.

  ### Linked List Allocator
  Maintains a free list of available memory regions. Freed blocks are returned to the list and
  reused by future allocations. The free list is kept sorted by address, enabling
  **coalescing** — adjacent freed regions are merged into a single larger block on every
  deallocation, reducing fragmentation. This goes beyond Opp's implementation which omits
  coalescing.

  ### Fixed Size Block Allocator
  Maintains separate free lists for fixed block sizes (8, 16, 32, 64, 128, 256, 512, 1024, 2048
  bytes). On allocation, the request is rounded up to the nearest block size and a block is
  popped off the corresponding free list in O(1). On deallocation, the block is pushed back onto its
  list for immediate reuse. Allocations larger than 2048 bytes fall back to the linked list
  allocator.
  
  The tradeoff versus the linked list allocator is speed for memory efficiency — rounding up to
fixed sizes introduces internal fragmentation, but eliminates free list searching entirely.

  ## Next Steps
  - Heap growth via `mmap` overprovisioning