// 导入ramdisk相关模块
#[cfg(feature = "ramdisk")]
pub mod ramdisk {
    pub use driver_common::{BaseDriverOps, DevError, DevResult, DeviceType};
    pub use driver_block::{BlockDriverOps, ramdisk::RamDisk};
    pub use ramdriver::{init_drivers, AllDevices, AxBlockDevice};
}

extern crate alloc;
pub mod sfs_apis;
pub mod img2sfs;

