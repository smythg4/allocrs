pub mod buddy;
pub mod bump;
pub mod fixed_size_block;
pub mod linked_list;
pub mod locked;

/// Pre-defined heap size
pub const HEAP_SIZE: usize = 1024 * 1024; // 1 MB heap

pub const PAGE_SIZE: usize = 1024 * 4; // 4 KB pages
