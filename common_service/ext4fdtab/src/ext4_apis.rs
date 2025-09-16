use std::path::PathBuf;
use as_std::libos::libos;
use spin::Mutex;
use alloc::sync::Arc;
use std::collections::HashMap;
use lazy_static::lazy_static;
use ext4_rs::{Ext4InodeRef, LinuxStat, InodeFileType};
use as_hostcall::{
    fdtab::{FdtabError, FdtabResult},
    types::{DirEntry, Fd, OpenFlags, OpenMode, Size, Stat},
};
use ext4_rs::*;

#[cfg(feature = "ramdisk")]
use crate::ramdisk::{BlockDriverOps, AxBlockDevice};

// RamDisk设备包装器，将ramdisk设备适配到ext4的Device trait
#[cfg(feature = "ramdisk")]
struct RamDiskDevice {
    device: spin::Mutex<AxBlockDevice>,
}

#[cfg(feature = "ramdisk")]
impl RamDiskDevice {
    fn new(device: AxBlockDevice) -> Self {
        Self { 
            device: spin::Mutex::new(device) 
        }
    }
}


impl BlockDevice for RamDiskDevice {
    fn read_offset(&self, offset: usize) -> Vec<u8> {
        let mut device = self.device.lock();
        let block_size = device.block_size();
        // 推测 ext4_rs 只会读取 1024 或 4096 字节
        // 如果 offset < 4096，返回 1024 字节（超级块）
        // 否则返回 4096 字节（数据块）
        let len = if offset == 1024 { 1024 } else { block_size };
        let mut buf = vec![0u8; len];
        let mut read = 0;
        while read < len {
            let abs_offset = offset + read;
            let block_id = abs_offset / block_size;
            let block_offset = abs_offset % block_size;
            let mut block_buf = vec![0u8; block_size];
            if device.read_block(block_id as u64, &mut block_buf).is_err() {
                eprintln!("[ramdisk] read_block failed at block_id {}", block_id);
                return vec![0u8; len];
            }
            let to_copy = std::cmp::min(len - read, block_size - block_offset);
            buf[read..read+to_copy].copy_from_slice(&block_buf[block_offset..block_offset+to_copy]);
            read += to_copy;
        }
        buf
    }
    fn write_offset(&self, offset: usize, data: &[u8]) {
        let mut device = self.device.lock();
        let block_size = device.block_size();
        let mut written = 0;
        let len = data.len();
        while written < len {
            let abs_offset = offset + written;
            let block_id = abs_offset / block_size;
            let block_offset = abs_offset % block_size;
            let mut block_buf = vec![0u8; block_size];
            // 先读出整个块
            if device.read_block(block_id as u64, &mut block_buf).is_err() {
                eprintln!("[ramdisk] read_block for write failed at block_id {}", block_id);
                return;
            }
            let to_copy = std::cmp::min(len - written, block_size - block_offset);
            block_buf[block_offset..block_offset+to_copy].copy_from_slice(&data[written..written+to_copy]);
            // 写回
            if device.write_block(block_id as u64, &block_buf).is_err() {
                eprintln!("[ramdisk] write_block failed at block_id {}", block_id);
                return;
            }
            written += to_copy;
        }
    }
}

fn get_fs_image_path() -> PathBuf {
    let image_path = match libos!(fs_image(as_std::init_context::isolation_ctx().isol_id)) {
        Some(s) => s,
        None => "fs_images/ext4.img".to_owned(),
    };

    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
        .join(image_path)
}

lazy_static! {
    static ref FD_TABLE: Mutex<HashMap<Fd, FileWrapper>> = Mutex::new(HashMap::new());
    static ref NEXT_FD: Mutex<Fd> = Mutex::new(3);
}

static mut GLOBAL_FS: Option<Arc<ext4_rs::Ext4>> = None;

#[cfg(feature = "lock")]
static GLOBAL_LOCK: Mutex<()> = Mutex::new(());


#[derive(Clone)]
struct FileWrapper {
    inode_ref: Ext4InodeRef,
    offset: usize,
    readable: bool,
    writable: bool,
}

impl FileWrapper {
    fn new(inode_ref: Ext4InodeRef, readable: bool, writable: bool) -> Self {
        FileWrapper { inode_ref, offset: 0, readable, writable }
    }

    fn read(&mut self, ext4: &Ext4, buf: &mut [u8]) -> Result<usize, Ext4Error> {
        if !self.readable { return Err(Ext4Error::new(Errno::EACCES)); }
        let len = ext4.read_at(self.inode_ref.inode_num, self.offset, buf)?;
        self.offset += len;
        Ok(len)
    }

    fn write(&mut self, ext4: &Ext4, buf: &[u8]) -> Result<usize, Ext4Error> {
        if !self.writable { return Err(Ext4Error::new(Errno::EACCES)); }
        let len = ext4.write_at(self.inode_ref.inode_num, self.offset, buf)?;
        self.offset += len;
        Ok(len)
    }

    fn seek(&mut self, pos: usize) { self.offset = pos; }

    fn stat(&self) -> LinuxStat {
        LinuxStat::from_inode_ref(&self.inode_ref)
    }
}

fn convert(stat: &LinuxStat) -> as_hostcall::types::Stat {
    as_hostcall::types::Stat {
        st_dev: stat.st_dev() as u64,
        st_ino: stat.st_ino() as u64,
        st_nlink: stat.st_nlink() as u64,
        st_mode: stat.st_mode() as core::ffi::c_uint,
        st_uid: stat.st_uid() as core::ffi::c_uint,
        st_gid: stat.st_gid() as core::ffi::c_uint,
        __pad0: 0,
        st_rdev: stat.st_rdev() as u64,
        st_size: stat.st_size() as usize,
        st_blksize: stat.st_blksize() as core::ffi::c_long,
        st_blocks: stat.st_blocks() as i64,
        st_atime: as_hostcall::types::TimeSpec {
            tv_sec: stat.st_atime() as i64,
            tv_nsec: stat.st_atime_nsec() as i64,
        },
        st_mtime: as_hostcall::types::TimeSpec {
            tv_sec: stat.st_mtime() as i64,
            tv_nsec: stat.st_mtime_nsec() as i64,
        },
        st_ctime: as_hostcall::types::TimeSpec {
            tv_sec: stat.st_ctime() as i64,
            tv_nsec: stat.st_ctime_nsec() as i64,
        },
        __unused: [0; 3],
    }
}

pub fn init() -> bool {

    let path_buf = get_fs_image_path();
    let image_path = path_buf.to_str().unwrap();

    #[cfg(feature = "ramdisk")]
    {
        
		// 初始化ramdisk驱动
        let mut all_devices = ramdriver::init_drivers(image_path);
        // 获取ramdisk设备
        if let Some(block_device) = all_devices.block.take_one() {
            
            // 计算ramdisk的总空间大小
            let device_size = block_device.size();
            println!("ext4fdtab::init: ramdisk size: {} bytes", device_size);
            // 创建ramdisk设备包装器
            let device = Arc::new(RamDiskDevice::new(block_device));
            // 用ext4_rs的Ext4::open挂载ext4文件系统
            let ext4 = Arc::new(ext4_rs::Ext4::open(device));

            unsafe {
                GLOBAL_FS = Some(ext4);
            }
            println!("ext4fdtab::init: global Ext4 FS initialized successfully");
            return true;
        } else {
            println!("ext4fdtab::init: no block device found");
            return false;
        }
    }
    
    #[cfg(not(feature = "ramdisk"))]
    {
        println!("ext4fdtab::init: ramdisk feature not enabled");
        return false;
    }




}

lazy_static::lazy_static! {
    static ref INIT_DONE: bool = {
        init()
    };
}

// 获取全局ext4文件系统实例
pub fn get_global_ext4() -> Option<Arc<Ext4>> {
    unsafe { GLOBAL_FS.clone() }
}

// 分配文件描述符
fn alloc_fd(inode_ref: Ext4InodeRef, readable: bool, writable: bool) -> Fd {
    let mut table = FD_TABLE.lock();
    let mut next = NEXT_FD.lock();
    let fd = *next;
    *next += 1;
    table.insert(fd, FileWrapper::new(inode_ref, readable, writable));
    fd
}

// 获取文件包装器
fn get_file_wrapper(fd: Fd) -> Option<FileWrapper> {
    FD_TABLE.lock().get(&fd).cloned()
}

// 移除文件描述符
fn remove_fd(fd: Fd) {
    FD_TABLE.lock().remove(&fd);
}


// 文件系统接口
// open
#[no_mangle]
pub fn open(path: &str, flags: OpenFlags, mode: OpenMode) -> FdtabResult<Fd> {
    let _ = *INIT_DONE;
    #[cfg(feature = "lock")]
    let _lock = GLOBAL_LOCK.lock();

    let ext4 = get_global_ext4().ok_or(FdtabError::FsError("global fs not initialized".to_string()))?;
    let readable = mode.contains(OpenMode::RD) || mode.contains(OpenMode::RDWR);
    let writable = mode.contains(OpenMode::WR) || mode.contains(OpenMode::RDWR);
    // 临时用"r"，可根据flags/mode完善
    let flags_str = "r";
    let inode_num = ext4.ext4_file_open(path, flags_str)
        .map_err(|e| FdtabError::Ext4Error(format!("open failed: {:?}", e)))?;
    let inode_ref = ext4.get_inode_ref(inode_num);
    let fd = alloc_fd(inode_ref, readable, writable);
    Ok(fd)
}

// read
#[no_mangle]
pub fn read(fd: Fd, buf: &mut [u8]) -> FdtabResult<Size> {
    let _ = *INIT_DONE;
    #[cfg(feature = "lock")]
    let _lock = GLOBAL_LOCK.lock();
    let mut fdtab = FD_TABLE.lock();
    if let Some(file_wrapper) = fdtab.get_mut(&fd) {
        file_wrapper.read(get_global_ext4().as_ref().unwrap(), buf)
            .map_err(|e| FdtabError::Ext4Error(format!("read failed: {:?}", e)))
            .map(Size::from)
    } else {
        Err(FdtabError::FsError("invalid fd".into()))
    }
}

// stat
#[no_mangle]
pub fn stat(fd: Fd) -> FdtabResult<Stat> {
    let fdtab = FD_TABLE.lock();
    if let Some(file_wrapper) = fdtab.get(&fd) {
        let stat = convert(&file_wrapper.stat());
        Ok(stat)
    } else {
        Err(FdtabError::FsError("invalid fd".into()))
    }
}

// write
#[no_mangle]
pub fn write(fd: Fd, buf: &[u8]) -> FdtabResult<Size> {
    let _ = *INIT_DONE;
    #[cfg(feature = "lock")]
    let _lock = GLOBAL_LOCK.lock();

    // 处理标准输出和标准错误
    if fd == 1 || fd == 2 {
        println!("ext4_apis::write: writing to stdout/stderr, calling libos!(stdout)");
        let result = libos!(stdout(buf));
        println!("ext4_apis::write: stdout result: {}", result);
        return Ok(Size::from(result));
    }

    let mut fdtab = FD_TABLE.lock();
    if let Some(file_wrapper) = fdtab.get_mut(&fd) {
        file_wrapper.write(get_global_ext4().as_ref().unwrap(), buf)
            .map_err(|e| FdtabError::Ext4Error(format!("write failed: {:?}", e)))
            .map(Size::from)
    } else {
        Err(FdtabError::FsError("invalid fd".into()))
    }
}

// close
#[no_mangle]
pub fn close(fd: Fd) -> FdtabResult<()> {
    let _ = *INIT_DONE;
    #[cfg(feature = "lock")]
    let _lock = GLOBAL_LOCK.lock();
    remove_fd(fd);
    Ok(())
}

// lseek
#[no_mangle]
pub fn lseek(fd: Fd, pos: u32) -> FdtabResult<()> {
    let _ = *INIT_DONE;
    #[cfg(feature = "lock")]
    let _lock = GLOBAL_LOCK.lock();
    let mut fdtab = FD_TABLE.lock();
    if let Some(file_wrapper) = fdtab.get_mut(&fd) {
        file_wrapper.seek(pos as usize);
        Ok(())
    } else {
        Err(FdtabError::FsError("invalid fd".into()))
    }
}

// readdir
#[no_mangle]
pub fn readdir(path: &str) -> FdtabResult<Vec<DirEntry>> {
    let _ = *INIT_DONE;
    #[cfg(feature = "lock")]
    let _lock = GLOBAL_LOCK.lock();
    let ext4 = get_global_ext4().ok_or(FdtabError::FsError("global fs not initialized".to_string()))?;
    let dir_inode_num = ext4.ext4_dir_open(path)
        .map_err(|e| FdtabError::Ext4Error(format!("readdir failed: {:?}", e)))?;
    let entries = ext4.ext4_dir_get_entries(dir_inode_num);
    let mut result = Vec::new();
    for entry in entries {
        result.push(DirEntry {
            dir_path: path.to_string(),
            entry_name: entry.get_name(),
            entry_type: entry.get_de_type() as u32,
        });
    }
    Ok(result)
}