use as_hostcall::{
    fdtab::{FdtabError, FdtabResult},
    types::{Fd, OpenFlags, OpenMode, Stat},
};

use crate::{
    io::{Read, Write},
    libos::libos,
    println,
};

pub struct File {
    raw_fd: Fd,
}

impl File {
    pub fn create(p: &str) -> Result<Self, FdtabError> {
        let flags = OpenFlags::O_CREAT;
        let mode = OpenMode::RDWR;
        println!("File::create: calling libos!(open) for path: {}", p);
        let raw_fd = libos!(open(p, flags, mode))?;
        println!("File::create: received fd: {}", raw_fd);
        println!("create file: {} okok", p);

        Ok(File { raw_fd })
    }

    pub fn open(p: &str) -> Result<Self, FdtabError> {
        let mode = OpenMode::RD;
        let flags = OpenFlags::empty();
        println!("open file: {} siki", p);
        let raw_fd = libos!(open(p, flags, mode))?;
        println!("open file: {} okok", p);

        Ok(File { raw_fd })
    }

    pub fn seek(&mut self, pos: u32) -> Result<(), FdtabError> {
        libos!(lseek(self.raw_fd, pos))
    }

    pub fn as_raw_fd(&self) -> Fd {
        self.raw_fd
    }

    pub fn from_raw_fd(fd: Fd) -> Self {
        Self { raw_fd: fd }
    }

    pub fn metadata(&self) -> FdtabResult<Stat> {
        libos!(stat(self.raw_fd))
    }

    pub fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.write_all(s.as_bytes()).map_err(|_| core::fmt::Error)
    }
}

impl Write for File {
    fn write(&mut self, buf: &[u8]) -> Result<usize, FdtabError> {
        println!("File::write: calling libos!(write) with fd: {}", self.raw_fd);
        let result = libos!(write(self.raw_fd, buf));
        println!("File::write: received result: {:?}", result);
        result
    }
}

impl Read for File {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, FdtabError> {
        libos!(read(self.raw_fd, buf))
    }
}

impl Drop for File {
    fn drop(&mut self) {
        libos!(close(self.raw_fd)).expect("close failed");
    }
}
