//! Semaphore

use crate::sync::UPSafeCell;
use crate::task::{
    block_current_and_run_next, current_process, current_task, wakeup_task, TaskControlBlock,
};
use alloc::{collections::VecDeque, sync::Arc};

/// semaphore structure
pub struct Semaphore {
    /// semaphore inner
    pub inner: UPSafeCell<SemaphoreInner>,
}

pub struct SemaphoreInner {
    pub count: isize,
    rid: usize,
    pub wait_queue: VecDeque<Arc<TaskControlBlock>>,
}

impl Semaphore {
    /// Create a new semaphore
    pub fn new(res_count: usize) -> Self {
        trace!("kernel: Semaphore::new");
        let rid = current_process()
            .inner_exclusive_access()
            .alloc_rid(res_count);
        Self {
            inner: unsafe {
                UPSafeCell::new(SemaphoreInner {
                    count: res_count as isize,
                    rid,
                    wait_queue: VecDeque::new(),
                })
            },
        }
    }

    /// up operation of semaphore
    pub fn up(&self) {
        trace!("kernel: Semaphore::up");
        let mut inner = self.inner.exclusive_access();
        // 恢复自己的 Allocation
        let process = current_process();
        let mut process_inner = process.inner_exclusive_access();
        let tid = current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid;
        let rid = inner.rid;
        process_inner.ba_allocation[tid].as_mut().unwrap()[rid] -= 1;

        inner.count += 1;
        debug!(
            "[semup] tid: {}, rid: {}, count: {} -> {}",
            tid,
            rid,
            inner.count - 1,
            inner.count
        );
        if inner.count <= 0 {
            if let Some(task) = inner.wait_queue.pop_front() {
                wakeup_task(task);
            }
        } else {
            *process_inner.ba_available[rid].as_mut().unwrap() += 1;
        }
    }

    /// down operation of semaphore
    pub fn down(&self) -> bool {
        trace!("kernel: Semaphore::down");
        let mut inner = self.inner.exclusive_access();
        let process = current_process();
        let mut process_inner = process.inner_exclusive_access();
        let tid = current_task()
            .unwrap()
            .inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .tid;
        // need 的第一维在创建线程的时候已经扩充好了
        let rid = inner.rid;
        // 修改 Need 数组
        process_inner.ba_need[tid].as_mut().unwrap()[rid] += 1;

        if !process_inner.deadlock_check() {
            warn!(
                "[sys_semaphore_down] tid: {}, rid: {} - deadlock detected.",
                tid, rid
            );
            return false;
        }
        drop(process_inner);
        drop(process);

        inner.count -= 1;
        let after_count  = inner.count;
        debug!(
            "[semdown] tid: {}, rid: {}, count: {} -> {}",
            tid,
            rid,
            inner.count + 1,
            inner.count
        );
        if inner.count < 0 {
            inner.wait_queue.push_back(current_task().unwrap());
            drop(inner);
            block_current_and_run_next();
        }

        // 从别的地方调度回来了, 说明现在能 -1 了
        // 修改 Allocation 数组, 恢复对 Need 的修改
        let process = current_process();
        let mut process_inner = process.inner_exclusive_access();
        process_inner.ba_need[tid].as_mut().unwrap()[rid] -= 1;
        process_inner.ba_allocation[tid].as_mut().unwrap()[rid] += 1;

        if after_count >= 0 {
            *process_inner.ba_available[rid].as_mut().unwrap() -= 1;
        }

        true
    }
}
