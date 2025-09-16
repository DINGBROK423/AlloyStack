/* rcorefdtab 模块入口，集成rcorefs文件系统 */
extern crate alloc;

pub mod rc_apis;

pub mod img2ramfs;

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use as_hostcall::types::{OpenFlags, OpenMode, Size};

//     #[test]
//     fn test_file_wrapper_creation() {
//         // 测试FileWrapper创建
//         println!("Testing FileWrapper creation...");
        
//         // 初始化文件系统
//         let init_result = rc_apis::init();
//         println!("File system init result: {}", init_result);
//         assert!(init_result);
        
//         println!("File system initialized successfully");
//     }

//     #[test]
//     fn test_basic_file_operations() {
//         // 初始化文件系统
//         assert!(rc_apis::init());
        
//         println!("Testing basic file operations...");
        
//         // 测试打开文件（只读模式）
//         let fd = rc_apis::open("/test.txt", OpenFlags::empty(), OpenMode::RD);
//         println!("Open result: {:?}", fd);
        
//         // 如果文件不存在，这是预期的错误
//         if fd.is_err() {
//             println!("File does not exist, which is expected for this test");
//             return;
//         }
        
//         let fd = fd.unwrap();
//         println!("File opened with fd: {}", fd);
        
//         // 测试关闭
//         let close_result = rc_apis::close(fd);
//         println!("Close result: {:?}", close_result);
//         assert!(close_result.is_ok());
//     }
// }
