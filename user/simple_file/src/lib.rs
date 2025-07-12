#![no_std]

extern crate alloc;

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

#[allow(unused_imports)]
use as_std::{
    agent::FaaSFuncResult as Result,
    fs::File,
    io::{Read, Write},
    println,
    time::SystemTime,
};

// fn init_input_file() {
//     File::create("fake_data_0.txt")
//         .unwrap()
//         .write_str(include_str!("../../../image_content/fake_data_0.txt"))
//         .unwrap();

//     File::create("fake_data_1.txt")
//         .unwrap()
//         .write_str(include_str!("../../../image_content/fake_data_1.txt"))
//         .unwrap();

//     File::create("fake_data_2.txt")
//         .unwrap()
//         .write_str(include_str!("../../../image_content/fake_data_2.txt"))
//         .unwrap();

//     File::create("fake_data_3.txt")
//         .unwrap()
//         .write_str(include_str!("../../../image_content/fake_data_3.txt"))
//         .unwrap();

//     File::create("fake_data_4.txt")
//         .unwrap()
//         .write_str(include_str!("../../../image_content/fake_data_4.txt"))
//         .unwrap();

//     let content = include_str!("../../../image_content/sort_data_0.txt");
//     File::create("sort_data_0.txt")
//         .unwrap()
//         .write_str(content)
//         .unwrap();

//     let start = SystemTime::now();
//     let mut array: Vec<i32> = Vec::new();
//     for num in content.split(',') {
//         let num = num.trim();
//         if num.is_empty() {
//             continue;
//         }
//         let num = num.parse().unwrap();
//         array.push(num);
//     }
//     println!(
//         "split {} numbers cost {}ms",
//         array.len(),
//         SystemTime::elapsed(&start).as_millis()
//     );

//     File::create("sort_data_1.txt")
//         .unwrap()
//         .write_str(include_str!("../../../image_content/sort_data_1.txt"))
//         .unwrap();

//     File::create("sort_data_2.txt")
//         .unwrap()
//         .write_str(include_str!("../../../image_content/sort_data_2.txt"))
//         .unwrap();

//     File::create("sort_data_3.txt")
//         .unwrap()
//         .write_str(include_str!("../../../image_content/sort_data_3.txt"))
//         .unwrap();

//     File::create("sort_data_4.txt")
//         .unwrap()
//         .write_str(include_str!("../../../image_content/sort_data_4.txt"))
//         .unwrap();
// }

#[no_mangle]
pub fn main() -> Result<()> {
    println!("main: starting simple_file application");
    // let start_time = SystemTime::now();
    let path = "lines.txt";
    println!("main: path = {}", path);

    /////////////////// test create/write/read. ///////////////////
    let data = "Rust LibOS Cool.";
    println!("main: about to call File::create");
    let mut output = File::create(path)?;
    println!("create file: {}", path);
    output.write_str(data).expect("");
    println!("write to file: {}", data);
    // drop(output);

    let mut input_file = File::open(path)?;
    println!("open file: {}", path);
    let mut file_content_buf = Vec::new();
    input_file
        .read_to_end(&mut file_content_buf)
        .expect("read failed");
    println!("read file: {} bytes", file_content_buf.len());

    let file_content = String::from_utf8_lossy(&file_content_buf).to_string();
    println!("file_content: {}", file_content);
    // println!("expect: {}", data);

    assert_eq!(file_content, data);

    /////////////////// test seek. ///////////////////
    input_file.seek(0)?;
    println!("seek to 0");
    file_content_buf.clear();
    input_file
        .read_to_end(&mut file_content_buf)
        .expect("read failed");
    println!("read after seek: {} bytes", file_content_buf.len());

    assert_eq!(
        file_content,
        String::from_utf8_lossy(&file_content_buf).to_string()
    );

    /////////////////// test seek. ///////////////////
    let meta = input_file.metadata().unwrap();
    println!("file metadata: st_size={} expect_size={}", meta.st_size, file_content.len());
    if meta.st_size != file_content.len() {
        Err("seek failed")?
    }

    // init_input_file();

    // println!(
    //     "simple_file exec: {}ms",
    //     SystemTime::elapsed(&start_time).as_millis()
    // );
    Ok(().into())
}
