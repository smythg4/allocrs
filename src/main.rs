use std::time::Instant;

use allocrs::buddy::BuddyAllocator;
use allocrs::fixed_size_block::FixedSizeBlockAllocator;
use allocrs::linked_list::LinkedListAllocator;
use allocrs::locked::Locked;

#[global_allocator]
//pub static GLOBAL_ALLOCATOR: Locked<FixedSizeBlockAllocator> = Locked::new(FixedSizeBlockAllocator::new());
//pub static GLOBAL_ALLOCATOR: Locked<LinkedListAllocator> = Locked::new(LinkedListAllocator::new());
pub static GLOBAL_ALLOCATOR: Locked<BuddyAllocator> = Locked::new(BuddyAllocator::new());

const ITERATIONS: usize = 100_000;

fn bench_many_small(label: &str) {
    let start = Instant::now();
    for i in 0..ITERATIONS {
        let x = Box::new(i);
        let _ = *x;
    }
    println!("{label}: many_small took {:?}", start.elapsed());
}

fn bench_long_lived(label: &str) {
    let start = Instant::now();
    let long_lived = Box::new(1usize);
    for i in 0..ITERATIONS {
        let x = Box::new(i);
        let _ = *x;
    }
    let _ = *long_lived;
    println!("{label}: long_lived took {:?}", start.elapsed());
}

fn bench_mixed_sizes(label: &str) {
    let start = Instant::now();
    for i in 0..ITERATIONS {
        match i % 4 {
            0 => {
                let _ = Box::new(i as u8);
            }
            1 => {
                let _ = Box::new(i as u32);
            }
            2 => {
                let _ = Box::new([i; 32]);
            }
            _ => {
                let _ = Box::new([i; 256]);
            }
        }
    }
    println!("{label}: mixed_sizes took {:?}", start.elapsed());
}

fn main() {
    bench_many_small("allocator");
    bench_long_lived("allocator");
    bench_mixed_sizes("allocator");
}
