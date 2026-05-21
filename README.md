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

  ## Next Steps
  - Fixed-size block allocator
  - Heap growth via `mmap` overprovisioning