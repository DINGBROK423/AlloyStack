use std::{
    fs::{self, File},
    io::{self, Read, Seek, Write},
};

use driver_common::{BaseDriverOps, DevError, DevResult, DeviceType};
use fscommon::BufStream;

use crate::BlockDriverOps;

const BLOCK_SIZE: usize = 4096; // 匹配SFS的块大小

/// A RAM disk that stores data in a BufStream.
pub struct RamDisk {
    inner: BufStream<File>,
    size: usize,
}

impl RamDisk {
    pub fn new(image: &str) -> Self {
        println!("RamDisk::new: trying to open image file: {}", image);
        
        let size_hint = match fs::metadata(image) {
            Ok(metadata) => {
                println!("RamDisk::new: file metadata ok, size: {}", metadata.len());
                metadata.len() as usize
            }
            Err(e) => {
                println!("RamDisk::new: failed to get file metadata: {:?}", e);
                panic!("failed to get file metadata: {:?}", e);
            }
        };
        
        let size = align_up(size_hint);
        println!("RamDisk::new: aligned size: {}", size);

        let mut config = std::fs::File::options();
        let image_file = match config.read(true).write(true).open(image) {
            Ok(file) => {
                println!("RamDisk::new: file opened successfully");
                file
            }
            Err(e) => {
                println!("RamDisk::new: failed to open file: {:?}", e);
                panic!("failed to open file: {:?}", e);
            }
        };

        let inner = BufStream::new(image_file);
        println!("RamDisk::new: BufStream created successfully");
        Self { inner, size }
    }

    /// Returns the size of the RAM disk in bytes.
    pub const fn size(&self) -> usize {
        self.size
    }
}

impl const BaseDriverOps for RamDisk {
    fn device_type(&self) -> DeviceType {
        DeviceType::Block
    }

    fn device_name(&self) -> &str {
        "ramdisk"
    }
}

impl BlockDriverOps for RamDisk {
    #[inline]
    fn num_blocks(&self) -> u64 {
        (self.size / BLOCK_SIZE) as u64
    }

    #[inline]
    fn block_size(&self) -> usize {
        BLOCK_SIZE
    }

    fn read_block(&mut self, block_id: u64, buf: &mut [u8]) -> DevResult {
        let offset = block_id as usize * BLOCK_SIZE;
        if offset + buf.len() > self.size {
            return Err(DevError::Io);
        }
        if buf.len() % BLOCK_SIZE != 0 {
            return Err(DevError::InvalidParam);
        }
        // buf.copy_from_slice(&self.data[offset..offset + buf.len()]);
        self.inner
            .seek(io::SeekFrom::Start(offset as u64))
            .map_err(|_| DevError::Io)?;
        self.inner.read(buf).map_err(|_| DevError::Io)?;
        Ok(())
    }

    fn write_block(&mut self, block_id: u64, buf: &[u8]) -> DevResult {
        let offset = block_id as usize * BLOCK_SIZE;
        if offset + buf.len() > self.size {
            println!("RamDisk::write_block: offset {} + buf.len {} > size {}", offset, buf.len(), self.size);
            return Err(DevError::Io);
        }
        if buf.len() % BLOCK_SIZE != 0 {
            println!("RamDisk::write_block: buf.len {} not aligned to BLOCK_SIZE {}", buf.len(), BLOCK_SIZE);
            return Err(DevError::InvalidParam);
        }
        // self.data[offset..offset + buf.len()].copy_from_slice(buf);
        println!("RamDisk::write_block: writing block {} at offset {} with {} bytes", block_id, offset, buf.len());
        self.inner
            .seek(io::SeekFrom::Start(offset as u64))
            .map_err(|e| {
                println!("RamDisk::write_block: seek failed: {:?}", e);
                DevError::Io
            })?;
        self.inner.write(buf).map_err(|e| {
            println!("RamDisk::write_block: write failed: {:?}", e);
            DevError::Io
        })?;
        // 确保数据被写入到文件
        self.inner.flush().map_err(|e| {
            println!("RamDisk::write_block: flush failed: {:?}", e);
            DevError::Io
        })?;
        println!("RamDisk::write_block: successfully wrote block {}", block_id);
        Ok(())
    }

    fn flush(&mut self) -> DevResult {
        Ok(())
    }
}

impl Default for RamDisk {
    fn default() -> Self {
        Self {
            size: Default::default(),
            inner: unimplemented!(),
        }
    }
}

const fn align_up(val: usize) -> usize {
    (val + BLOCK_SIZE - 1) & !(BLOCK_SIZE - 1)
}
