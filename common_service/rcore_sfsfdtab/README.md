# rcore_sfsfdtab

这是一个基于rcore-fs的文件系统模块，集成了ramdisk支持。

## 功能特性

- 支持SFS（Simple File System）文件系统
- 集成ramdisk块设备驱动
- 提供标准的文件系统API接口
- 支持文件读写、目录操作等

## 编译选项

### 启用ramdisk功能

在Cargo.toml中启用ramdisk特性：

```toml
[dependencies]
rcorefdtab = { path = "./common_service/rcore_sfsfdtab", features = ["ramdisk"] }
```

或者在编译时指定：

```bash
cargo build --features ramdisk
```

### 可用的特性

- `ramdisk`: 启用ramdisk块设备支持
- `std`: 启用标准库支持
- `lock`: 启用文件系统锁

## 使用方法

### 初始化文件系统

```rust
use rcorefdtab::sfs_apis;

// 初始化文件系统（会自动初始化ramdisk）
let success = sfs_apis::init();
if success {
    println!("文件系统初始化成功");
} else {
    println!("文件系统初始化失败");
}
```

### 文件操作

```rust
use rcorefdtab::sfs_apis;
use as_hostcall::types::{OpenFlags, OpenMode};

// 打开文件
let fd = sfs_apis::open(
    "/test.txt", 
    OpenFlags::O_CREAT | OpenFlags::O_RDWR,
    OpenMode::RDWR
).unwrap();

// 写入数据
let data = b"Hello, World!";
sfs_apis::write(fd, data).unwrap();

// 读取数据
let mut buf = [0u8; 13];
sfs_apis::read(fd, &mut buf).unwrap();

// 关闭文件
sfs_apis::close(fd).unwrap();
```

### 目录操作

```rust
use rcorefdtab::sfs_apis;

// 读取目录
let entries = sfs_apis::readdir("/").unwrap();
for entry in entries {
    println!("文件: {}, 类型: {:?}", entry.name, entry.file_type);
}
```

## ramdisk模块结构

```
ramdisk/
├── driver_common/     # 通用驱动接口
├── driver_block/      # 块设备驱动接口
└── ramdriver/         # ramdisk驱动实现
```

### 主要组件

- **driver_common**: 提供基础的设备驱动接口（BaseDriverOps）
- **driver_block**: 提供块设备专用接口（BlockDriverOps）
- **ramdriver**: 实现ramdisk设备驱动，支持文件镜像和内存存储

### ramdisk特性

- 支持从文件镜像创建ramdisk
- 支持内存中的ramdisk存储
- 块大小：512字节
- 支持多级块寻址（直接块、间接块、二级间接块）
- 最大文件大小：4GB

## 测试

运行ramdisk集成测试：

```rust
#[cfg(feature = "ramdisk")]
{
    let success = rcorefdtab::test_ramdisk_integration();
    if success {
        println!("ramdisk集成测试通过");
    } else {
        println!("ramdisk集成测试失败");
    }
}
```

## 注意事项

1. 启用ramdisk特性时，需要确保有足够的系统内存
2. 文件系统镜像路径通过环境变量或默认路径指定
3. 首次使用时会自动创建SFS文件系统
4. 支持热插拔ramdisk设备

## 许可证

本项目采用MulanPSL-2.0许可证。 