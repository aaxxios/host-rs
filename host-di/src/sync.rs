use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, ThreadId};

/// Serialises construction of cached instances.
///
/// Re-entrant so a constructor can resolve further cached dependencies on the
/// same thread. Holding one lock per owner (container or scope) means a cycle
/// that only shows up at resolution time is always walked by a single thread,
/// where the resolver chain detects it, instead of deadlocking two threads.
#[derive(Default)]
pub(crate) struct ConstructionLock {
    owner: Mutex<Owner>,
    released: Condvar,
}

#[derive(Default)]
struct Owner {
    thread: Option<ThreadId>,
    depth: usize,
}

impl ConstructionLock {
    pub(crate) fn lock(&self) -> ConstructionGuard<'_> {
        let me = thread::current().id();
        let mut owner = self.owner();
        loop {
            match owner.thread {
                None => {
                    owner.thread = Some(me);
                    break;
                }
                Some(thread) if thread == me => break,
                Some(_) => {
                    owner = self
                        .released
                        .wait(owner)
                        .unwrap_or_else(PoisonError::into_inner);
                }
            }
        }
        owner.depth += 1;
        ConstructionGuard { lock: self }
    }

    fn owner(&self) -> MutexGuard<'_, Owner> {
        self.owner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub(crate) struct ConstructionGuard<'a> {
    lock: &'a ConstructionLock,
}

impl Drop for ConstructionGuard<'_> {
    fn drop(&mut self) {
        let mut owner = self.lock.owner();
        owner.depth -= 1;
        if owner.depth == 0 {
            owner.thread = None;
            self.lock.released.notify_all();
        }
    }
}
