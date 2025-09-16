use std::fs::File;
use std::io::Read;
use alloc::sync::Arc;
use rcore_fs::vfs::{FileType, INode, FsError, FileSystem};
use fatfs::{FileSystem as FatFileSystem, FsOptions, Dir};
use chrono;

/// 递归解包 fatfs 目录到 sfs 目录
fn unpack_dir_to_sfs(
    sfs_dir: Arc<dyn INode>,
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
            println!("DIR  : {}", full_path);
            // 先查找是否已存在
            let new_dir = match sfs_dir.create(&name, FileType::Dir, 0o755) {
                Ok(d) => d,
                Err(FsError::EntryExist) => {
                    println!("  目录已存在，跳过: {}", full_path);
                    match sfs_dir.find(&name) {
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
            if let Err(e) = unpack_dir_to_sfs(new_dir, entry.to_dir(), &full_path) {
                println!("  递归解包目录失败: {} {:?}", full_path, e);
            }
        } else if entry.is_file() {
            println!("FILE : {} [size={}]", full_path, entry.len());

            // 先查找是否已存在
            let new_file = match sfs_dir.create(&name, FileType::File, 0o644) {
                Ok(f) => f,
                Err(FsError::EntryExist) => {
                    println!("  文件已存在，跳过: {}", full_path);
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
            // 调整文件大小并写入内容
            if let Err(e) = new_file.resize(buf.len()) {
                println!("  调整文件大小失败: {} {:?}", full_path, e);
                continue;
            }
            if let Err(e) = new_file.write_at(0, &buf) {
                println!("  写入SFS失败: {} {:?}", full_path, e);
            }
            println!("  写入SFS成功");
        }
    }
    Ok(())
}

/// 总入口：把 fatfs.img 解包到 sfs 根目录
pub fn img_to_sfs_bridge(root_inode: Arc<dyn INode>, img_path: &str) {
    let img_file = match File::open(img_path) {
        Ok(f) => f,
        Err(e) => {
            println!("read img file failed: {:?}", e);
            return;
        }
    };
    let fs = match FatFileSystem::new(img_file, FsOptions::new()) {
        Ok(fs) => fs,
        Err(e) => {
            println!("fatfs parse failed: {:?}", e);
            return;
        }
    };
    let root_dir = fs.root_dir();
    if let Err(e) = unpack_dir_to_sfs(root_inode, root_dir, "") {
        println!("unpack fatfs.img to sfs failed: {:?}", e);
    }
    // // 新增：生成 SFS 镜像
    // let sfs_img_dir = "/home/tank/alloystack/AlloyStack/fs_images";
    // let sfs_img_name = format!("sfsfs.img");
    // let sfs_img_path = format!("{}/{}", sfs_img_dir, sfs_img_name);
    // let sfs_img_size =  1024* 1024 * 1024; // 1024MB，可根据需要调整
    // fatfs_img_to_sfs_img(img_path, &sfs_img_path, sfs_img_size);
}

// /// 将 fatfs 镜像解包到新建的 sfs 镜像文件，并保存到指定目录
// pub fn fatfs_img_to_sfs_img(fatfs_img_path: &str, sfs_img_path: &str, sfs_img_size: usize) {
//     use std::fs::{create_dir_all, OpenOptions};
//     use std::io::Write;
//     use std::sync::Mutex;
//     use rcore_fs_sfs::SimpleFileSystem;
//     use alloc::sync::Arc;

//     // 保证输出目录存在
//     if let Some(parent) = std::path::Path::new(sfs_img_path).parent() {
//         if let Err(e) = create_dir_all(parent) {
//             println!("创建输出目录失败: {:?}", e);
//             return;
//         }
//     }

//     // 创建指定大小的 sfs 镜像文件
//     let mut sfs_file = match OpenOptions::new().read(true).write(true).create(true).truncate(true).open(sfs_img_path) {
//         Ok(f) => f,
//         Err(e) => {
//             println!("创建 SFS 镜像文件失败: {:?}", e);
//             return;
//         }
//     };
//     if let Err(e) = sfs_file.set_len(sfs_img_size as u64) {
//         println!("设置 SFS 镜像文件大小失败: {:?}", e);
//         return;
//     }

//     let sfs_file = Arc::new(Mutex::new(sfs_file));
//     let sfs = match SimpleFileSystem::create(sfs_file.clone(), sfs_img_size) {
//         Ok(fs) => fs,
//         Err(e) => {
//             println!("创建 SFS 文件系统失败: {:?}", e);
//             return;
//         }
//     };
//     let sfs_root = sfs.root_inode();

//     // 打开 fatfs 镜像
//     let fatfs_file = match File::open(fatfs_img_path) {
//         Ok(f) => f,
//         Err(e) => {
//             println!("打开 FATFS 镜像失败: {:?}", e);
//             return;
//         }
//     };
//     let fatfs = match FatFileSystem::new(fatfs_file, FsOptions::new()) {
//         Ok(fs) => fs,
//         Err(e) => {
//             println!("解析 FATFS 镜像失败: {:?}", e);
//             return;
//         }
//     };
//     let fat_root = fatfs.root_dir();

//     // 解包
//     if let Err(e) = unpack_dir_to_sfs(sfs_root, fat_root, "") {
//         println!("解包 FATFS 到 SFS 失败: {:?}", e);
//         return;
//     }

//     // 同步 SFS 文件系统到磁盘
//     if let Err(e) = sfs.sync() {
//         println!("同步 SFS 文件系统失败: {:?}", e);
//         return;
//     }
//     println!("SFS 镜像已生成: {}", sfs_img_path);
// } 