# allocrs
Building custom memory allocators in Rust, following Philipp Oppermann's [_Writing an OS in
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

### Buddy Allocator
Manages memory as a binary tree of power-of-two-sized blocks. The heap is divided into two
equal top-level regions; each can be recursively split in half down to a configurable minimum
block size. On allocation, the request is rounded up to the nearest power of two and the
smallest fitting free block is returned, splitting larger blocks as needed. On deallocation,
a freed block is merged with its **buddy** — the adjacent same-sized block it was split from
— and the merge recurses up the tree until the buddy is occupied or the top level is reached.

The buddy address is computed with an XOR relative to the heap base:
```
buddy = heap_start + ((addr - heap_start) ^ block_size)
```

`MIN_BLOCK_SIZE` and `ORDERS` are const generic parameters on the struct, so the
configuration is fixed at compile time with no runtime overhead:

```rust
Locked<BuddyAllocator<64, 14>>   // 64-byte min block, 14 orders → 1 MB heap
Locked<BuddyAllocator<4096, 8>>  // 4 KB min block,   8 orders  → 1 MB heap
```

The caller must satisfy `MIN_BLOCK_SIZE << ORDERS == HEAP_SIZE`; this is validated
with a runtime assert in `init()`. A page-sized minimum (4 KB) is appropriate when the
buddy allocator serves as a page allocator underneath a slab layer; a cache-line-sized
minimum (64 bytes) is appropriate when used directly as a global allocator.

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
Locked<BuddyAllocator<MIN_BLOCK_SIZE, ORDERS>>
SlabCache<T>
  ├── partial: linked list of slabs
  ├── full:    linked list of slabs
  └── empty:   linked list of slabs
```
Each allocator is backed by an anonymous `mmap` region acquired lazily on first allocation.

## Deviations from Oppermann

**macOS userspace instead of bare metal x86_64**
Oppermann's allocators are initialized by `init_heap`, which manually maps virtual addresses to
physical frames through page tables. On macOS in userspace the kernel handles it with `mmap`,
which requests a committed virtual address range directly from the OS.

**Lazy `mmap` initialization**
Each allocator self-initializes on its first allocation call by checking an uninitialized
sentinel and calling `mmap` if needed.

**Generic `Locked<A>` wrapper**
Oppermann implements `Locked<A>` using the `spin` crate. This project implements the
spinlock directly using `AtomicBool` and `UnsafeCell`, with a RAII `LockGuard<A>` that
releases the lock on drop. The wrapper is fully generic and reused across all allocators.

**Const generic `BuddyAllocator<MIN_BLOCK_SIZE, ORDERS>`**
Rather than fixing the block size and order count as module-level constants, the buddy
allocator is parameterised at the type level. This allows different configurations to
coexist as distinct types with no runtime overhead, and makes misconfiguration visible
at the call site.

**Linked list allocator with coalescing**
Oppermann's linked list allocator does not coalesce adjacent free regions. This implementation
keeps the free list sorted by address and merges adjacent blocks on every deallocation, reducing
fragmentation.

### Slab Allocator
Manages memory as a collection of fixed-size object caches. Each cache maintains three
linked lists of slabs — **partial** (some slots used), **full** (all slots used), and
**empty** (no slots used) — enabling O(1) allocation and efficient identification of pages
that can be returned to the OS.

`Slab::next` is an intrusive linked list pointer owned and managed entirely by `SlabCache`.
Slabs carry their own list linkage rather than requiring a separate container, following
the same pattern used for free lists throughout this project.

A `Slab` is backed by a single mmap'd page divided into fixed-size slots. Free slots form
an embedded linked list (the same in-place `ListNode` trick used throughout this project).
The minimum slot size is `max(size_of::<T>(), size_of::<ListNode>())` so that free slots
can always hold a list pointer regardless of `T`.

`SlabCache<T>` owns slabs as `Box<Slab<T>>` and routes each slab between the three lists
as its occupancy changes. Deallocation walks the lists to find the owning slab via a
`contains()` check, frees the slot, then re-routes the slab based on its new state.

Unlike the other allocators, `SlabCache` is not a `#[global_allocator]` — it is a
purpose-built object cache intended to sit above a page allocator such as the buddy
allocator.

## Next Steps
- Heap growth via `mmap` overprovisioning