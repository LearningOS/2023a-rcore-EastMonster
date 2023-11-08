//! Process management syscalls
use core::mem::size_of;
use crate::{
    config::MAX_SYSCALL_NUM,
    mm::{translated_byte_buffer, VirtAddr, MapPermission},
    task::{
        change_program_brk, current_user_token, exit_current_and_run_next,
        suspend_current_and_run_next, TaskStatus, get_current_task_info, current_map_area, current_check_allocated, current_check_unallocated, current_unmap_area
    },
    timer::{get_time_us, get_time_ms},
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

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    let mut buffers = translated_byte_buffer(current_user_token(), _ts as *const u8, size_of::<TimeVal>());
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
    trace!("kernel: sys_task_info NOT IMPLEMENTED YET!");
    let mut buffers = translated_byte_buffer(current_user_token(), _ti as *const u8, size_of::<TaskInfo>());
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
        return -1; // not implemented yet
    }
    0
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    trace!("kernel: sys_mmap");
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

// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    trace!("kernel: sys_munmap NOT IMPLEMENTED YET!");
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
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
