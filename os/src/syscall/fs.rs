//! File and filesystem-related syscalls
use core::mem::size_of;

use crate::fs::{open_file, OpenFlags, Stat, has_file, linkat, inode_status, unlinkat};
use crate::mm::{translated_byte_buffer, translated_str, UserBuffer};
use crate::task::{current_task, current_user_token};

pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_write", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        if !file.writable() {
            return -1;
        }
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.write(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_read", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        if !file.readable() {
            return -1;
        }
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        trace!("kernel: sys_read .. file.read");
        file.read(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_open(path: *const u8, flags: u32) -> isize {
    trace!("kernel:pid[{}] sys_open", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(inode) = open_file(path.as_str(), OpenFlags::from_bits(flags).unwrap()) {
        let mut inner = task.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        fd as isize
    } else {
        -1
    }
}

pub fn sys_close(fd: usize) -> isize {
    trace!("kernel:pid[{}] sys_close", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    inner.fd_table[fd].take();
    0
}

/// YOUR JOB: Implement fstat.
pub fn sys_fstat(_fd: usize, _st: *mut Stat) -> isize {
    trace!("kernel:pid[{}] sys_fstat", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let mut buffers = translated_byte_buffer(current_user_token(), _st as *const u8, size_of::<Stat>());
    assert!(!buffers.is_empty());
    
    if buffers.len() == 1 {
        let inner = task.inner_exclusive_access();
        if let Some(fd) = inner.fd_table[_fd].clone() {
            let _buf = buffers.get_mut(0).unwrap();
            let stat = inode_status(fd);
            unsafe { core::ptr::write(_buf.as_mut_ptr() as *mut Stat, stat); }

            0
        } else {
            warn!("[sys_fstat] fd does not exist.");
            -1
        }
    } else {
        warn!("[sys_fstat] splited!");
        -1
    }
}

/// YOUR JOB: Implement linkat.
pub fn sys_linkat(_old_name: *const u8, _new_name: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_linkat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();

    let old_name = translated_str(token, _old_name);
    if !has_file(&old_name) {
        warn!("[sys_linkat] invalid _old_name.");
        return -1;
    }

    let new_name = translated_str(token, _new_name);
    if old_name == new_name {
        warn!("[sys_linkat] trying to link to a file with the same name!");
        return -1;
    }

    linkat(&old_name, &new_name);
    
    0
}

/// YOUR JOB: Implement unlinkat.
pub fn sys_unlinkat(_name: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_unlinkat NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let path = translated_str(current_user_token(), _name);
    if !has_file(&path) {
        warn!("[sys_unlinkat] invalid path.");
        return -1;
    }

    let nlink = unlinkat(&path);
    if nlink == 0 {
        info!("[sys_unlinkat] nlink = 0, inode released.");
    }

    0
}
