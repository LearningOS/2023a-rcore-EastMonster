//! Process management syscalls
//!
use core::mem::size_of;

use alloc::sync::Arc;

use crate::{
    config::{BIG_STRIDE, MAX_SYSCALL_NUM},
    fs::{open_file, OpenFlags},
    mm::{translated_byte_buffer, translated_refmut, translated_str, MapPermission, VirtAddr},
    task::{
        add_task, current_check_allocated, current_check_unallocated, current_map_area,
        current_task, current_unmap_area, current_user_token, exit_current_and_run_next,
        get_current_task_info, suspend_current_and_run_next, TaskStatus,
    },
    timer::{get_time_ms, get_time_us},
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// Task information
#[allow(dead_code)]
pub struct TaskInfo {
    /// Task status in it's life cycle
    status: TaskStatus,
    /// The numbers of syscall called by task
    syscall_times: [u32; MAX_SYSCALL_NUM],
    /// Total running time of task
    time: usize,
}

pub fn sys_exit(exit_code: i32) -> ! {
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

pub fn sys_yield() -> isize {
    //trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

pub fn sys_getpid() -> isize {
    trace!("kernel: sys_getpid pid:{}", current_task().unwrap().pid.0);
    current_task().unwrap().pid.0 as isize
}

pub fn sys_fork() -> isize {
    trace!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    // add new task to scheduler
    add_task(new_task);
    new_pid as isize
}

pub fn sys_exec(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let task = current_task().unwrap();
        task.exec(all_data.as_slice());
        0
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    //trace!("kernel: sys_waitpid");
    let task = current_task().unwrap();
    // find a child process

    // ---- access current PCB exclusively
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after being removed from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child PCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB automatically
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel:pid[{}] sys_get_time", current_task().unwrap().pid.0);
    let mut buffers =
        translated_byte_buffer(current_user_token(), _ts as *const u8, size_of::<TimeVal>());
    let time_us = get_time_us();

    if buffers.is_empty() {
        return -1;
    }

    let _t_s = time_us / 1_000_000;
    let _t_us = time_us % 1_000_000;
    if buffers.len() == 1 {
        let buffer = buffers.get_mut(0).unwrap();
        let ts = TimeVal {
            sec: _t_s,
            usec: _t_us,
        };
        unsafe {
            // 参考: ChatGPT, 如何像 C 语言一样转成指定指针然后写数据
            core::ptr::write(buffer.as_mut_ptr() as *mut TimeVal, ts); // buffer 前多加了个解引用就挂了...
        }
    } else {
        // 被放在两页了. 一个变量不会跨页?
        let _t_s_ptr = &_t_s as *const usize as *const u8;
        let _t_us_ptr = &_t_us as *const usize as *const u8;
        unsafe {
            let buf_p1 = buffers.get_mut(0).unwrap();
            core::ptr::write(buf_p1.as_mut_ptr() as *mut usize, _t_s);
            let buf_p2 = buffers.get_mut(1).unwrap();
            core::ptr::write(buf_p2.as_mut_ptr() as *mut usize, _t_us);
        }
    }

    0
}

/// YOUR JOB: Finish sys_task_info to pass testcases
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TaskInfo`] is splitted by two pages ?
pub fn sys_task_info(_ti: *mut TaskInfo) -> isize {
    trace!(
        "kernel:pid[{}] sys_task_info",
        current_task().unwrap().pid.0
    );
    let mut buffers = translated_byte_buffer(
        current_user_token(),
        _ti as *const u8,
        size_of::<TaskInfo>(),
    );
    debug!("[sys_task_info]: buffers.len() = {}", buffers.len());
    if buffers.is_empty() {
        return -1;
    }

    if buffers.len() == 1 {
        let tinfo = get_current_task_info();
        let buf = buffers.get_mut(0).unwrap();
        let new_ti = TaskInfo {
            status: tinfo.0,
            syscall_times: tinfo.1,
            time: get_time_ms() - tinfo.2,
        };
        unsafe {
            core::ptr::write(buf.as_mut_ptr() as *mut TaskInfo, new_ti);
        }
    } else {
        error!("TaskInfo is splited! not implemented yet.");
        return -1;
    }
    0
}

/// YOUR JOB: Implement mmap.
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    trace!("kernel:pid[{}] sys_mmap", current_task().unwrap().pid.0);
    // 检查参数合法性
    let start: VirtAddr = _start.into();
    if !start.aligned() {
        warn!("[sys_mmap] Virtual address is not aligned.");
        return -1;
    }
    if _port & !0x7 != 0 {
        warn!("[sys_mmap] _port[63:3] != 0.");
        return -1;
    }
    if _port & 0x7 == 0 {
        warn!("[sys_mmap] _port[2:0] == 0.");
        return -1;
    }
    if _len == 0 {
        info!("[sys_mmap] _len == 0.");
        return 0;
    }
    let end: VirtAddr = (_start + _len).into();
    // 检查是否存在已分配页
    if current_check_allocated(start, end) {
        warn!("[sys_mmap] Detected an allocated page in given range.");
        return -1;
    }

    let mut permission = MapPermission::U;
    permission |= MapPermission::from_bits((_port << 1) as u8 & 7).unwrap();
    current_map_area(start, end, permission);

    0
}

/// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    trace!("kernel:pid[{}] sys_munmap", current_task().unwrap().pid.0);
    let start: VirtAddr = _start.into();
    if !start.aligned() {
        warn!("[sys_munmap] Virtual address is not aligned.");
        return -1;
    }
    if _len == 0 {
        return 0;
    }
    let end: VirtAddr = (_start + _len).into();

    if current_check_unallocated(start, end) {
        warn!("[sys_munmap] Detected an unallocated page in given range.");
        return -1;
    }

    current_unmap_area(start, end);

    0
}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel:pid[{}] sys_sbrk", current_task().unwrap().pid.0);
    if let Some(old_brk) = current_task().unwrap().change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

/// YOUR JOB: Implement spawn.
/// HINT: fork + exec =/= spawn
pub fn sys_spawn(_path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_spawn", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, _path);
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let task = current_task().unwrap();
        let new_task = task.spawn(all_data.as_slice());
        let pid = new_task.getpid();
        add_task(new_task);

        pid as isize
    } else {
        warn!("[sys_spawn] failed.");
        -1
    }
}

// YOUR JOB: Set task priority.
pub fn sys_set_priority(_prio: isize) -> isize {
    trace!("kernel:pid[{}] sys_set_priority NOT IMPLEMENTED", current_task().unwrap().pid.0);
    if _prio <= 1 {
        warn!("[sys_set_priority] _prio <= 1");
        -1
    } else {
        let task = current_task().unwrap();
        let mut inner = task.inner_exclusive_access();
        inner.priority = _prio as usize;
        inner.pass = BIG_STRIDE / inner.priority;
        inner.priority as isize
    }
}
