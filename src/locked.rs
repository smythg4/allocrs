use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};

pub struct Locked<A> {
    inner: UnsafeCell<A>,
    lock: AtomicBool,
}

impl<A> Locked<A> {
    pub const fn new(inner: A) -> Self {
        Locked {
            inner: UnsafeCell::new(inner),
            lock: AtomicBool::new(false),
        }
    }

    pub fn lock(&self) -> LockGuard<'_, A> {
        while self
            .lock
            .compare_exchange(false, true, Acquire, Relaxed)
            .is_err()
        {
            // Spin until the lock is acquired
            std::hint::spin_loop();
        }
        LockGuard { locked: self }
    }
}

unsafe impl<A> Sync for Locked<A> {}

pub struct LockGuard<'a, A> {
    locked: &'a Locked<A>,
}

impl<'a, A> Drop for LockGuard<'a, A> {
    fn drop(&mut self) {
        self.locked.lock.store(false, Release)
    }
}

impl<'a, A> Deref for LockGuard<'a, A> {
    type Target = A;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.locked.inner.get() }
    }
}

impl<'a, A> DerefMut for LockGuard<'a, A> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.locked.inner.get() }
    }
}