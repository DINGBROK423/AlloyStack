use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use alloc::sync::Arc;
use rcore_fs::vfs::{FileType, INode, FsError};
use fatfs::{FileSystem, FsOptions, Dir, DirEntry};

/// 递归解包 fatfs 目录到 ramfs 目录
fn unpack_dir_to_ramfs(
    ramfs_dir: Arc<dyn INode>,
    fat_dir: Dir<File>,
    prefix: &str,
) -> Result<(), FsError> {
    for entry in fat_dir.iter() {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                println!("读取目录项失败: {:?}，跳过", e);
                continue;
            }
        };
        let name = entry.file_name();
        // 跳过 . 和 ..
        if name == "." || name == ".." {
            continue;
        }
        let full_path = format!("{}/{}", prefix, name);

        if entry.is_dir() {
            // println!("DIR  : {}", full_path);
            // 先查找是否已存在
            let new_dir = match ramfs_dir.create(&name, FileType::Dir, 0o755) {
                Ok(d) => d,
                Err(FsError::EntryExist) => {
                    // println!("  目录已存在，跳过: {}", full_path);
                    match ramfs_dir.find(&name) {
                        Ok(d) => d,
                        Err(e) => {
                            println!("  查找已存在目录失败: {} {:?}", full_path, e);
                            continue;
                        }
                    }
                }
                Err(e) => {
                    println!("  创建目录失败: {} {:?}", full_path, e);
                    continue;
                }
            };
            // 递归
            if let Err(e) = unpack_dir_to_ramfs(new_dir, entry.to_dir(), &full_path) {
                println!("  递归解包目录失败: {} {:?}", full_path, e);
            }
        } else if entry.is_file() {
            // println!("FILE : {} [size={}]", full_path, entry.len());
            // 限制单文件最大读取大小，防止 OOM
            if entry.len() > 100 * 1024 * 1024 {
                println!("  文件过大，跳过: {}", full_path);
                continue;
            }
            // 先查找是否已存在
            let new_file = match ramfs_dir.create(&name, FileType::File, 0o644) {
                Ok(f) => f,
                Err(FsError::EntryExist) => {
                    // println!("  文件已存在，跳过: {}", full_path);
                    continue;
                }
                Err(e) => {
                    println!("  创建文件失败: {} {:?}", full_path, e);
                    continue;
                }
            };
            let mut fat_file = entry.to_file();
            let mut buf = Vec::with_capacity(entry.len() as usize);
            if let Err(e) = fat_file.read_to_end(&mut buf) {
                println!("  读取文件内容失败: {} {:?}", full_path, e);
                continue;
            }
            if let Err(e) = new_file.write_at(0, &buf) {
                println!("  写入RAMFS失败: {} {:?}", full_path, e);
            }
            // println!("  写入RAMFSok");
        }
    }
    Ok(())
}

/// 总入口：把 fatfs.img 解包到 ramfs 根目录
pub fn img_to_ramfs_bridge(root_inode: Arc<dyn INode>, img_path: &str) {
    let img_file = match File::open(img_path) {
        Ok(f) => f,
        Err(e) => {
            println!("read img file failed: {:?}", e);
            return;
        }
    };
    let fs = match FileSystem::new(img_file, FsOptions::new()) {
        Ok(fs) => fs,
        Err(e) => {
            println!("fatfs parse failed: {:?}", e);
            return;
        }
    };
    let root_dir = fs.root_dir();
    if let Err(e) = unpack_dir_to_ramfs(root_inode, root_dir, "") {
        println!("unpack fatfs.img to ramfs failed: {:?}", e);
    }
}
