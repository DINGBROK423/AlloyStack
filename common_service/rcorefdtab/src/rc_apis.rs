use std::path::PathBuf;
use as_std::libos::libos;
use spin::Mutex;
use alloc::sync::Arc;
use rcore_fs::vfs::{FileSystem, FileType};
use rcore_fs_mountfs::MountFS;
use rcore_fs_ramfs::RamFS;
use crate::img2ramfs;

use as_hostcall::{
    fdtab::{FdtabError, FdtabResult},
    types::{DirEntry, Fd, OpenFlags, OpenMode, Size, Stat},
};

use alloc::collections::BTreeMap;

// 封装rcore-fs的File，添加偏移量管理
struct FileWrapper {
    inode: Arc<dyn rcore_fs::vfs::INode>,
    offset: usize,
    readable: bool,
    writable: bool,
}

impl FileWrapper {
    fn new(inode: Arc<dyn rcore_fs::vfs::INode>, readable: bool, writable: bool) -> Self {
        FileWrapper {
            inode,
            offset: 0,
            readable,
            writable,
        }
    }

    fn read(&mut self, buf: &mut [u8]) -> Result<usize, rcore_fs::vfs::FsError> {
        if !self.readable {
            return Err(rcore_fs::vfs::FsError::InvalidParam);
        }
        let len = self.inode.read_at(self.offset, buf)?;
        self.offset += len;
        Ok(len)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize, rcore_fs::vfs::FsError> {
        if !self.writable {
            println!("FileWrapper::write: not writable");
            return Err(rcore_fs::vfs::FsError::InvalidParam);
        }
        println!("FileWrapper::write: writing {} bytes at offset {}", buf.len(), self.offset);
        let len = self.inode.write_at(self.offset, buf)?;
        println!("FileWrapper::write: wrote {} bytes successfully", len);
        self.offset += len;
        Ok(len)
    }

    fn seek(&mut self, pos: usize) -> Result<(), rcore_fs::vfs::FsError> {
        self.offset = pos;
        Ok(())
    }

    fn metadata(&self) -> Result<rcore_fs::vfs::Metadata, rcore_fs::vfs::FsError> {
        self.inode.metadata()
    }

    fn get_offset(&self) -> usize {
        self.offset
    }

    fn get_inode(&self) -> Arc<dyn rcore_fs::vfs::INode> {
        self.inode.clone()
    }
}

impl Clone for FileWrapper {
    fn clone(&self) -> Self {
        FileWrapper {
            inode: self.inode.clone(),
            offset: self.offset,
            readable: self.readable,
            writable: self.writable,
        }
    }
}

lazy_static::lazy_static! {
    static ref FD_TABLE: Mutex<BTreeMap<Fd, FileWrapper>> = Mutex::new(BTreeMap::new());
    static ref NEXT_FD: Mutex<Fd> = Mutex::new(3); // 0/1/2保留
}


static mut GLOBAL_FS: Option<Arc<MountFS>> = None;


#[cfg(feature = "lock")]
static GLOBAL_LOCK: Mutex<()> = Mutex::new(());

fn convert(meta: &rcore_fs::vfs::Metadata) -> as_hostcall::types::Stat {
    as_hostcall::types::Stat {
        st_dev: meta.dev as u64,
        st_ino: meta.inode as u64,
        st_nlink: meta.nlinks as u64,
        st_mode: meta.mode as core::ffi::c_uint,
        st_uid: meta.uid as core::ffi::c_uint,
        st_gid: meta.gid as core::ffi::c_uint,
        __pad0: 0, // rcore-fs没有此字段，补0
        st_rdev: meta.rdev as u64,
        st_size: meta.size as usize,
        st_blksize: meta.blk_size as core::ffi::c_long,
        st_blocks: meta.blocks as i64,
        st_atime: as_hostcall::types::TimeSpec {
            tv_sec: meta.atime.sec,
            tv_nsec: meta.atime.nsec as i64,
        },
        st_mtime: as_hostcall::types::TimeSpec {
            tv_sec: meta.mtime.sec,
            tv_nsec: meta.mtime.nsec as i64,
        },
        st_ctime: as_hostcall::types::TimeSpec {
            tv_sec: meta.ctime.sec,
            tv_nsec: meta.ctime.nsec as i64,
        },
        __unused: [0; 3], // rcore-fs没有此字段，补0
    }
}

fn get_fs_image_path() -> PathBuf {
    let image_path = match libos!(fs_image(as_std::init_context::isolation_ctx().isol_id)) {
        Some(s) => s,
        None => "fs_images/fatfs.img".to_owned(),
    };

    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
        .join(image_path)
}

// 初始化文件系统
pub fn init() -> bool {
    let ramfs = RamFS::new();
    let mntfs = MountFS::new(ramfs);
    let root = mntfs.root_inode();

    // 1. 读取镜像并写入ramfs
    let img_path = get_fs_image_path();
    img2ramfs::img_to_ramfs_bridge(root.clone(), img_path.to_str().unwrap());

    unsafe {
        GLOBAL_FS = Some(mntfs.clone());
    }
    
    // 2. 初始化标准输入输出
    // 创建stdin (fd 0)
    let stdin_inode = root.lookup("/dev/stdin").unwrap_or_else(|_| root.clone());
    FD_TABLE.lock().insert(0, FileWrapper::new(stdin_inode, true, false));
    
    // 创建stdout (fd 1)
    let stdout_inode = root.lookup("/dev/stdout").unwrap_or_else(|_| root.clone());
    FD_TABLE.lock().insert(1, FileWrapper::new(stdout_inode, false, true));
    
    // 创建stderr (fd 2)
    let stderr_inode = root.lookup("/dev/stderr").unwrap_or_else(|_| root.clone());
    FD_TABLE.lock().insert(2, FileWrapper::new(stderr_inode, false, true));
    
    println!("rcore-fs ramfs initialized successfully");
    println!("rcorefdtab: initialized stdin(fd=0), stdout(fd=1), stderr(fd=2)");
    true
}

    
lazy_static::lazy_static! {
    static ref INIT_DONE: bool = {
        init()
    };
}


// 获取全局文件系统实例
pub fn get_global_fs() -> Option<Arc<dyn FileSystem>> {
    unsafe { GLOBAL_FS.clone().map(|mntfs| mntfs as Arc<dyn FileSystem>) }
}


fn alloc_fd(inode: Arc<dyn rcore_fs::vfs::INode>, readable: bool, writable: bool) -> Fd {
    let mut table = FD_TABLE.lock();
    let mut next = NEXT_FD.lock();
    let fd = *next;
    *next += 1;
    table.insert(fd, FileWrapper::new(inode, readable, writable));
    println!("alloc_fd: allocated fd {} for inode", fd);
    println!("alloc_fd: FD_TABLE now has {} entries", table.len());
    fd
}

fn get_file_wrapper(fd: Fd) -> Option<FileWrapper> {
    FD_TABLE.lock().get(&fd).cloned()
}



fn remove_fd(fd: Fd) {
    FD_TABLE.lock().remove(&fd);
}

#[no_mangle]
pub fn open(path: &str, flags: OpenFlags, mode: OpenMode) -> FdtabResult<Fd> {
    let _ = *INIT_DONE;
    #[cfg(feature = "lock")]
    let _lock = GLOBAL_LOCK.lock();
    
    let root = match get_global_fs() {
        Some(fs) => fs.root_inode(),
        None => {
            println!("GLOBAL_FS 未初始化1！");
            return Err(FdtabError::FsError("global fs not initialized".to_string()));
        }
    };
    
    println!("try to open file: {} with flags: {:?}, mode: {:?}", path, flags.bits(), mode.bits());
    
    // 确定读写权限
    let readable = mode.contains(OpenMode::RD) || mode.contains(OpenMode::RDWR);
    let writable = mode.contains(OpenMode::WR) || mode.contains(OpenMode::RDWR);
    
    // 统一用 lookup，支持绝对/相对路径
    match root.lookup(path) {
        Ok(inode) => {
            if let Ok(meta) = inode.metadata() {
                println!("open ok: inode meta = {:?}", meta);
            }
            println!("open: calling alloc_fd with readable={}, writable={}", readable, writable);
            let fd = alloc_fd(inode, readable, writable);
            println!("open: allocated fd {}", fd);
            Ok(fd)
        },
        Err(e) => {
            println!("file not found: {}, err={:?}", path, e);
            Err(FdtabError::FsError(format!("file not found: {}", path)))
        }
    }
}

#[no_mangle]
pub fn read(fd: Fd, buf: &mut [u8]) -> FdtabResult<Size> {
    let _ = *INIT_DONE;
    #[cfg(feature = "lock")]
    let _lock = GLOBAL_LOCK.lock();
    let mut fdtab = FD_TABLE.lock();
    if let Some(file_wrapper) = fdtab.get_mut(&fd) {
        file_wrapper.read(buf)
            .map_err(|e| FdtabError::FsError(format!("read failed: {:?}", e)))
            .map(|len| Size::from(len))
    } else {
        Err(FdtabError::FsError("invalid fd".into()))
    }
}

#[no_mangle]
pub fn write(fd: Fd, buf: &[u8]) -> FdtabResult<Size> {
    let _ = *INIT_DONE;
    #[cfg(feature = "lock")]
    let _lock = GLOBAL_LOCK.lock();
    println!("rc_apis::write: starting write for fd {}", fd);
    
    // 处理标准输出和标准错误
    if fd == 1 || fd == 2 {
        println!("rc_apis::write: writing to stdout/stderr, calling libos!(stdout)");
        let result = libos!(stdout(buf));
        println!("rc_apis::write: stdout result: {}", result);
        return Ok(Size::from(result));
    }
    
    let mut fdtab = FD_TABLE.lock();
    println!("rc_apis::write: FD_TABLE has {} entries", fdtab.len());
    println!("rc_apis::write: FD_TABLE keys: {:?}", fdtab.keys().collect::<Vec<_>>());
    if let Some(file_wrapper) = fdtab.get_mut(&fd) {
        println!("rc_apis::write: found file_wrapper for fd {}", fd);
        let result = file_wrapper.write(buf)
            .map_err(|e| FdtabError::FsError(format!("write failed: {:?}", e)))
            .map(|len| Size::from(len));
        println!("rc_apis::write: write result: {:?}", result);
        result
    } else {
        println!("rc_apis::write: invalid fd {}", fd);
        Err(FdtabError::FsError("invalid fd".into()))
    }
}

#[no_mangle]
pub fn close(fd: Fd) -> FdtabResult<()> {
    let _ = *INIT_DONE;
    #[cfg(feature = "lock")]
    let _lock = GLOBAL_LOCK.lock();
    remove_fd(fd);
    Ok(())
}

#[no_mangle]
pub fn lseek(fd: Fd, pos: u32) -> FdtabResult<()> {
    let _ = *INIT_DONE;
    #[cfg(feature = "lock")]
    let _lock = GLOBAL_LOCK.lock();
    let mut fdtab = FD_TABLE.lock();
    if let Some(file_wrapper) = fdtab.get_mut(&fd) {
        file_wrapper.seek(pos as usize)
            .map_err(|e| FdtabError::FsError(format!("lseek failed: {:?}", e)))
    } else {
        Err(FdtabError::FsError("invalid fd".into()))
    }
}

#[no_mangle]
pub fn stat(fd: Fd) -> FdtabResult<Stat> {
    let _ = *INIT_DONE;
    #[cfg(feature = "lock")]
    let _lock = GLOBAL_LOCK.lock();
    match get_file_wrapper(fd) {
        Some(file_wrapper) => {
            file_wrapper.metadata()
                .map(|m| convert(&m))
                .map_err(|e| FdtabError::FsError(format!("stat failed: {:?}", e)))
        },
        None => Err(FdtabError::FsError("invalid fd".into())),
    }
}

#[no_mangle]
pub fn readdir(path: &str) -> FdtabResult<Vec<DirEntry>> {
    let _ = *INIT_DONE;
    #[cfg(feature = "lock")]
    let _lock = GLOBAL_LOCK.lock();
    let root = match get_global_fs() {
        Some(fs) => fs.root_inode(),
        None => {
            println!("GLOBAL_FS 未初始化2！");
            return Err(FdtabError::FsError("global fs not initialized".to_string()));
        }
    };
    let inode = if path.starts_with('/') {
        root.lookup(path)
    } else {
        root.find(path)
    };
    match inode {
        Ok(dir) => {
            let mut entries = Vec::new();
            match dir.list() {
                Ok(names) => {
                    for name in names {
                        if let Ok(child) = dir.find(&name) {
                            if let Ok(meta) = child.metadata() {
                                entries.push(DirEntry {
                                    dir_path: path.to_string(),
                                    entry_name: name,
                                    entry_type: meta.type_ as u32,
                                });
                            }
                        }
                    }
                    Ok(entries)
                }
                Err(e) => Err(FdtabError::FsError(format!("readdir failed: {:?}", e))),
            }
        }
        Err(e) => Err(FdtabError::FsError(format!("readdir failed: {:?}", e))),
    }
}

#[no_mangle]
pub fn create(parent_fd: usize, name: &str, type_: FileType, mode: u32) -> Option<usize> {
    let _ = *INIT_DONE;
    #[cfg(feature = "lock")]
    let _lock = GLOBAL_LOCK.lock();
    let fdtab = FD_TABLE.lock();
    println!("create my wife");
    if let Some(parent_wrapper) = fdtab.get(&(parent_fd as u32)) {
        let parent_inode = parent_wrapper.get_inode();
        // 必须是目录
        if let Ok(meta) = parent_inode.metadata() {
            if meta.type_ != FileType::Dir {
                return None;
            }
        } else {
            return None;
        }
        // 创建新文件/目录
        if let Ok(new_inode) = parent_inode.create(name, type_, mode) {
            let new_fd = alloc_fd(new_inode, true, true);
            return Some(new_fd as usize);
        }
    }
    None
}

#[no_mangle]
pub fn link(parent_fd: usize, name: &str, target_fd: usize) -> bool {
    let _ = *INIT_DONE;
    #[cfg(feature = "lock")]
    let _lock = GLOBAL_LOCK.lock();
    let fdtab = FD_TABLE.lock();
    if let (Some(parent_wrapper), Some(target_wrapper)) = (fdtab.get(&(parent_fd as u32)), fdtab.get(&(target_fd as u32))) {
        let parent_inode = parent_wrapper.get_inode();
        let target_inode = target_wrapper.get_inode();
        if parent_inode.link(name, &target_inode).is_ok() {
            return true;
        }
    }
    false
}

#[no_mangle]
pub fn unlink(parent_fd: usize, name: &str) -> bool {
    let _ = *INIT_DONE;
    #[cfg(feature = "lock")]
    let _lock = GLOBAL_LOCK.lock();
    let fdtab = FD_TABLE.lock();
    if let Some(parent_wrapper) = fdtab.get(&(parent_fd as u32)) {
        let parent_inode = parent_wrapper.get_inode();
        if parent_inode.unlink(name).is_ok() {
            return true;
        }
    }
    false
}

#[no_mangle]
pub fn rename(old_fd: usize, new_parent_fd: usize, new_name: &str) -> bool {
    let _ = *INIT_DONE;
    #[cfg(feature = "lock")]
    let _lock = GLOBAL_LOCK.lock();
    let fdtab = FD_TABLE.lock();
    if let (Some(old_wrapper), Some(new_parent_wrapper)) = (fdtab.get(&(old_fd as u32)), fdtab.get(&(new_parent_fd as u32))) {
        let old_inode = old_wrapper.get_inode();
        let new_parent_inode = new_parent_wrapper.get_inode();
        if let Ok(meta) = old_inode.metadata() {
            if let Ok(_) = new_parent_inode.move_(&meta.inode.to_string(), &new_parent_inode, new_name) {
                return true;
            }
        }
    }
    false
}

#[no_mangle]
pub fn set_metadata(fd: usize, meta: &rcore_fs::vfs::Metadata) -> bool {
    let _ = *INIT_DONE;
    #[cfg(feature = "lock")]
    let _lock = GLOBAL_LOCK.lock();
    let fdtab = FD_TABLE.lock();
    if let Some(file_wrapper) = fdtab.get(&(fd as u32)) {
        let inode = file_wrapper.get_inode();
        if inode.set_metadata(meta).is_ok() {
            return true;
        }
    }
    false
}

#[no_mangle]
pub fn flush(fd: usize) -> bool {
    let _ = *INIT_DONE;
    #[cfg(feature = "lock")]
    let _lock = GLOBAL_LOCK.lock();
    let fdtab = FD_TABLE.lock();
    if let Some(file_wrapper) = fdtab.get(&(fd as u32)) {
        let inode = file_wrapper.get_inode();
        if inode.sync_all().is_ok() {
            return true;
        }
    }
    false
}

#[no_mangle]
pub fn sync(fd: usize) -> bool {
    let _ = *INIT_DONE;
    #[cfg(feature = "lock")]
    let _lock = GLOBAL_LOCK.lock();
    let fdtab = FD_TABLE.lock();
    if let Some(file_wrapper) = fdtab.get(&(fd as u32)) {
        let inode = file_wrapper.get_inode();
        if inode.sync_data().is_ok() {
            return true;
        }
    }
    false
}

#[no_mangle]
pub fn poll(fd: usize) -> Option<rcore_fs::vfs::PollStatus> {
    let _ = *INIT_DONE;
    #[cfg(feature = "lock")]
    let _lock = GLOBAL_LOCK.lock();
    let fdtab = FD_TABLE.lock();
    if let Some(file_wrapper) = fdtab.get(&(fd as u32)) {
        let inode = file_wrapper.get_inode();
        return inode.poll().ok();
    }
    None
}

#[no_mangle]
pub fn set_nonblocking(_fd: usize, _nonblocking: bool) -> bool {
    // rcore-fs INode trait 没有非阻塞接口，直接返回 true
    true
}

