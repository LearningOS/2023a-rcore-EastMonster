//! Mutex (spin-like and blocking(sleep))

use super::UPSafeCell;
use crate::task::{TaskControlBlock, current_process};
use crate::task::{block_current_and_run_next, suspend_current_and_run_next};
use crate::task::{current_task, wakeup_task};
use alloc::{collections::VecDeque, sync::Arc};

/// Mutex trait
pub trait Mutex: Sync + Send {
    /// Lock the mutex
    fn lock(&self) -> bool;
    /// Unlock the mutex
    fn unlock(&self);
}

/// Spinlock Mutex struct
pub struct MutexSpin {
    locked: UPSafeCell<bool>,
}

impl MutexSpin {
    /// Create a new spinlock mutex
    pub fn new() -> Self {
        Self {
            locked: unsafe { UPSafeCell::new(false) },
        }
    }
}

impl Mutex for MutexSpin {
    /// Lock the spinlock mutex
    fn lock(&self) -> bool {
        trace!("kernel: MutexSpin::lock");
        loop {
            let mut locked = self.locked.exclusive_access();
            if *locked {
                drop(locked);
                suspend_current_and_run_next();
                continue;
            } else {
                *locked = true;
                return true;
            }
        }
    }

    fn unlock(&self) {
        trace!("kernel: MutexSpin::unlock");
        let mut locked = self.locked.exclusive_access();
        *locked = false;
    }
}

/// Blocking Mutex struct
pub struct MutexBlocking {
    inner: UPSafeCell<MutexBlockingInner>,
}

pub struct MutexBlockingInner {
    locked: bool,
    rid: usize,
    wait_queue: VecDeque<Arc<TaskControlBlock>>,
}

impl MutexBlocking {
    /// Create a new blocking mutex
    pub fn new() -> Self {
        trace!("kernel: MutexBlocking::new");
        let rid = current_process().inner_exclusive_access().alloc_rid(1);
        Self {
            inner: unsafe {
                UPSafeCell::new(MutexBlockingInner {
                    locked: false,
                    rid,
                    wait_queue: VecDeque::new(),
                })
            },
        }
    }
}

impl Mutex for MutexBlocking {
    /// lock the blocking mutex
    fn lock(&self) -> bool {
        trace!("kernel: MutexBlocking::lock");
        let mut mutex_inner = self.inner.exclusive_access();
        let process = current_process();
        let mut process_inner = process.inner_exclusive_access();
        let tid = current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid;
        // need 的第一维在创建线程的时候已经扩充好了
        let rid = mutex_inner.rid;
        // 修改 Need 数组
        process_inner.ba_need[tid].as_mut().unwrap()[rid] += 1;

        if !process_inner.deadlock_check() {
            warn!("[sys_mutex_lock] deadlock detected.");
            return false;
        }
        drop(process_inner);
        drop(process);
        
        if mutex_inner.locked {
            mutex_inner.wait_queue.push_back(current_task().unwrap());
            drop(mutex_inner);
            block_current_and_run_next();
        } else {
            // 拿到的时候锁就是空的, 遂减少 available
            *current_process().inner_exclusive_access().ba_available[rid].as_mut().unwrap() -= 1;
            mutex_inner.locked = true;
        }
        // 从别的地方调度回来了或者直接通过了 if 到了这里, 说明锁现在在自己手上
        // 修改 Allocation 数组, 恢复对 Need 的修改
        let process = current_process();
        let mut process_inner = process.inner_exclusive_access();
        process_inner.ba_need[tid].as_mut().unwrap()[rid] -= 1;

        process_inner.ba_allocation[tid].as_mut().unwrap()[rid] += 1;
        assert!(process_inner.ba_allocation[tid].as_mut().unwrap()[rid] == 1); // 对互斥锁, allo 增加后应该是 1

        true
    }

    /// unlock the blocking mutex
    fn unlock(&self) {
        trace!("kernel: MutexBlocking::unlock");
        let mut mutex_inner = self.inner.exclusive_access();
        assert!(mutex_inner.locked);
        // 恢复自己的 Allocation
        let process = current_process();
        let mut process_inner = process.inner_exclusive_access();
        let tid = current_task().unwrap().inner_exclusive_access().res.as_ref().unwrap().tid;
        let rid = mutex_inner.rid;
        process_inner.ba_allocation[tid].as_mut().unwrap()[rid] -= 1;
        assert!(process_inner.ba_allocation[tid].as_mut().unwrap()[rid] == 0); // 对互斥锁, allo 恢复后应该是 0
        if let Some(waking_task) = mutex_inner.wait_queue.pop_front() {
            wakeup_task(waking_task);
        } else {
            // 没人要锁, 增加 Available
            mutex_inner.locked = false;
            *process_inner.ba_available[rid].as_mut().unwrap() += 1;
        }
    }
}
