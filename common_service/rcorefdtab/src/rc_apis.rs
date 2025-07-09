extern crate alloc;

use std::path::PathBuf;
use alloc::vec;
use as_std::libos::libos;
use spin::Mutex;
use alloc::sync::Arc;
use rcore_fs::vfs::{FileSystem, FileType};
use rcore_fs_mountfs::MountFS;
use rcore_fs_ramfs::RamFS;

use as_hostcall::{
    fdtab::{FdtabError, FdtabResult},
    types::{DirEntry, Fd, OpenFlags, OpenMode, Size, Stat, TimeSpec},
};

use alloc::collections::BTreeMap;

lazy_static::lazy_static! {
    static ref FD_TABLE: Mutex<BTreeMap<Fd, Arc<dyn rcore_fs::vfs::INode>>> = Mutex::new(BTreeMap::new());
    static ref NEXT_FD: Mutex<Fd> = Mutex::new(3); // 0/1/2保留
}

#[cfg(feature = "use-ramdisk")]
use rcore_fs_ramfs::RamFS;

static mut GLOBAL_FS: Option<Arc<MountFS>> = None;

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
    let ramfs = RamFS::new(); // Arc<RamFS>
    let main_fs: Arc<dyn FileSystem> = ramfs.clone() as Arc<dyn FileSystem>;
    let mntfs = MountFS::new(main_fs.clone());
    unsafe {
        GLOBAL_FS = Some(mntfs.clone());
    }
    println!("rcore-fs initialized successfully");
    true
}




// 获取全局文件系统实例
pub fn get_global_fs() -> Option<Arc<dyn FileSystem>> {
    unsafe { GLOBAL_FS.clone().map(|mntfs| mntfs as Arc<dyn FileSystem>) }
}

// 懒加载初始化
lazy_static::lazy_static! {
    static ref INIT_DONE: bool = {
        init()
    };
}

fn alloc_fd(inode: Arc<dyn rcore_fs::vfs::INode>) -> Fd {
    let mut table = FD_TABLE.lock();
    let mut next = NEXT_FD.lock();
    let fd = *next;
    *next += 1;
    table.insert(fd, inode);
    fd
}

fn get_inode(fd: Fd) -> Option<Arc<dyn rcore_fs::vfs::INode>> {
    FD_TABLE.lock().get(&fd).cloned()
}

fn remove_fd(fd: Fd) {
    FD_TABLE.lock().remove(&fd);
}

#[no_mangle]
pub fn open(path: &str, _flags: OpenFlags, _mode: OpenMode) -> FdtabResult<Fd> {
    let root = get_global_fs().unwrap().root_inode();
    let inode = if path.starts_with('/') {
        root.lookup(path)
    } else {
        root.find(path)
    };
    match inode {
        Ok(inode) => Ok(alloc_fd(inode)),
        Err(e) => Err(FdtabError::FsError(format!("open failed: {:?}", e))),
    }
}

#[no_mangle]
pub fn read(fd: Fd, buf: &mut [u8]) -> FdtabResult<Size> {
    match get_inode(fd) {
        Some(inode) => inode.read_at(0, buf).map_err(|e| FdtabError::FsError(format!("read failed: {:?}", e))),
        None => Err(FdtabError::FsError("invalid fd".into())),
    }
}

#[no_mangle]
pub fn write(fd: Fd, buf: &[u8]) -> FdtabResult<Size> {
    match get_inode(fd) {
        Some(inode) => inode.write_at(0, buf).map_err(|e| FdtabError::FsError(format!("write failed: {:?}", e))),
        None => Err(FdtabError::FsError("invalid fd".into())),
    }
}

#[no_mangle]
pub fn close(fd: Fd) -> FdtabResult<()> {
    remove_fd(fd);
    Ok(())
}

#[no_mangle]
pub fn lseek(_fd: Fd, _pos: u32) -> FdtabResult<()> {
    // RamFS/INode不支持lseek，直接返回Ok(())或Err
    Ok(())
}

#[no_mangle]
pub fn stat(fd: Fd) -> FdtabResult<Stat> {
    match get_inode(fd) {
        Some(inode) => inode.metadata().map(|m| convert(&m)).map_err(|e| FdtabError::FsError(format!("stat failed: {:?}", e))),
        None => Err(FdtabError::FsError("invalid fd".into())),
    }
}

#[no_mangle]
pub fn readdir(path: &str) -> FdtabResult<Vec<DirEntry>> {
    let root = get_global_fs().unwrap().root_inode();
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
    let fdtab = FD_TABLE.lock();
    if let Some(parent_inode) = fdtab.get(&(parent_fd as u32)) {
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
            let new_fd = alloc_fd(new_inode);
            return Some(new_fd as usize);
        }
    }
    None
}

#[no_mangle]
pub fn link(parent_fd: usize, name: &str, target_fd: usize) -> bool {
    let fdtab = FD_TABLE.lock();
    if let (Some(parent_inode), Some(target_inode)) = (fdtab.get(&(parent_fd as u32)), fdtab.get(&(target_fd as u32))) {
        if parent_inode.link(name, target_inode).is_ok() {
            return true;
        }
    }
    false
}

#[no_mangle]
pub fn unlink(parent_fd: usize, name: &str) -> bool {
    let fdtab = FD_TABLE.lock();
    if let Some(parent_inode) = fdtab.get(&(parent_fd as u32)) {
        if parent_inode.unlink(name).is_ok() {
            return true;
        }
    }
    false
}

#[no_mangle]
pub fn rename(old_fd: usize, new_parent_fd: usize, new_name: &str) -> bool {
    let fdtab = FD_TABLE.lock();
    if let (Some(old_inode), Some(new_parent_inode)) = (fdtab.get(&(old_fd as u32)), fdtab.get(&(new_parent_fd as u32))) {
        if let Ok(meta) = old_inode.metadata() {
            if let Ok(_) = new_parent_inode.move_(&meta.inode.to_string(), new_parent_inode, new_name) {
                return true;
            }
        }
    }
    false
}

#[no_mangle]
pub fn set_metadata(fd: usize, meta: &rcore_fs::vfs::Metadata) -> bool {
    let fdtab = FD_TABLE.lock();
    if let Some(inode) = fdtab.get(&(fd as u32)) {
        if inode.set_metadata(meta).is_ok() {
            return true;
        }
    }
    false
}

#[no_mangle]
pub fn flush(fd: usize) -> bool {
    let fdtab = FD_TABLE.lock();
    if let Some(inode) = fdtab.get(&(fd as u32)) {
        if inode.sync_all().is_ok() {
            return true;
        }
    }
    false
}

#[no_mangle]
pub fn sync(fd: usize) -> bool {
    let fdtab = FD_TABLE.lock();
    if let Some(inode) = fdtab.get(&(fd as u32)) {
        if inode.sync_data().is_ok() {
            return true;
        }
    }
    false
}

#[no_mangle]
pub fn poll(fd: usize) -> Option<rcore_fs::vfs::PollStatus> {
    let fdtab = FD_TABLE.lock();
    if let Some(inode) = fdtab.get(&(fd as u32)) {
        return inode.poll().ok();
    }
    None
}

#[no_mangle]
pub fn set_nonblocking(_fd: usize, _nonblocking: bool) -> bool {
    // rcore-fs INode trait 没有非阻塞接口，直接返回 true
    true
}

