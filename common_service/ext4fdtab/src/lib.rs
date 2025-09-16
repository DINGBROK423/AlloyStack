// 导入ramdisk相关模块（用于feature = "ramdisk"时）
#[cfg(feature = "ramdisk")]
pub mod ramdisk {
    pub use ramdriver::*;
    pub use driver_common::*;
    pub use driver_block::*;
}

// 导入ext4_apis模块
mod ext4_apis;
extern crate alloc;
// 重新导出ext4_apis中的所有对外接口
pub use ext4_apis::{
    open,
    read,
    write,
    close,
    lseek,
    stat,
    readdir,
};
