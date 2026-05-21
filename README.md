# allocrs
Building custom memory allocators in Rust, following Phil Opp's [_Writing an OS in
Rust_](https://os.phil-opp.com/allocator-designs/) blog — but adapted for macOS userspace
instead of bare metal x86_64.

## Design

All allocators are backed by anonymous `mmap` regions rather than a static buffer or a fixed
compile-time heap. Memory is acquired from the OS on first allocation and managed entirely by
the allocator from there.

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

## Architecture

`Locked<A>` is a generic spinlock wrapper that provides thread safety for any allocator `A`
without using `std::sync::Mutex`. It holds the allocator in an `UnsafeCell<A>` for interior
mutability and an `AtomicBool` as the lock. Acquiring the lock returns a `LockGuard<A>` that
releases it on drop.
```
Locked<BumpAllocator>
Locked<LinkedListAllocator>
Locked<FixedSizeBlockAllocator>
  └── fallback: LinkedListAllocator (unguarded, protected by outer lock)
```
Each allocator is backed by an anonymous `mmap` region acquired lazily on first allocation.

## Deviations from Opp

**macOS userspace instead of bare metal x86_64**
Opp's allocators are initialized by `init_heap`, which manually maps virtual addresses to
physical frames through page tables. On macOS in userspace the kernel handles that — the
equivalent is `mmap`, which requests a committed virtual address range directly from the OS.

**Lazy `mmap` initialization**
Opp calls `init_heap` explicitly at a known point during kernel boot. In userspace, Rust's
runtime makes allocations before `main` runs, so explicit initialization is too late. Each
allocator self-initializes on its first allocation call by checking an uninitialized sentinel
and calling `mmap` if needed.

**Generic `Locked<A>` wrapper**
Opp implements a bespoke `Locked<A>` using the `spin` crate. This project implements the
spinlock directly using `AtomicBool` and `UnsafeCell`, with a RAII `LockGuard<A>` that
releases the lock on drop. The wrapper is fully generic and reused across all three allocators.

**Linked list allocator with coalescing**
Opp's linked list allocator does not coalesce adjacent free regions. This implementation
keeps the free list sorted by address and merges adjacent blocks on every deallocation, reducing
fragmentation.

## Next Steps
- Slab Allocator
- Buddy Allocator
- Heap growth via `mmap` overprovisioning