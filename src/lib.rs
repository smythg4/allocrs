pub mod bump;
pub mod linked_list;
pub mod fixed_size_block;
pub mod locked;

/// Pre-defined heap size
pub const HEAP_SIZE: usize = 1024 * 1024; // 1 MB heap