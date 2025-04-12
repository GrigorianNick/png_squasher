use std::{fs::{self, File}, io::{BufWriter, Error}, path::PathBuf, thread};

use clap::Parser;
use png::EncodingError;

extern crate png;

#[derive(Parser, Debug)]
struct Args {
}

fn compress_file(infile_name: &String, outfile_name: &String) -> Result<(), Error> {
    println!("Compressing:{}...", infile_name);
    let decoder =    png::Decoder::new(std::fs::File::open(infile_name)?);
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf)?;
    let bytes = &buf[..info.buffer_size()];

    if let Ok(true) = fs::exists(outfile_name) {
        let mut perms = std::fs::metadata(outfile_name)?.permissions();
        if perms.readonly() {
            perms.set_readonly(false);
            std::fs::set_permissions(outfile_name, perms)?;
        }
    }
    let file = File::create(outfile_name)?;
    let ref mut w = BufWriter::new(file);
    
    let mut encoder = png::Encoder::new(w, info.width, info.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Best);
    let mut writer = encoder.write_header()?;

    Ok(writer.write_image_data(&bytes)?)
}

fn find_png_paths(path: &String) -> Vec<String>  {
    let res = std::fs::read_dir(path);
    if res.is_err() {
        return vec![];
    }
    let entries : Vec<PathBuf> = res.unwrap().filter_map(Result::ok).map(|entry| entry.path()).collect();
    let png_entries = entries.iter().filter_map(|entry| {
        if let Some("png") = entry.extension()?.to_str() {
            Some(entry)
        } else {
            None
        }
    }).map(|path| {
        path.as_os_str().to_string_lossy().to_string()
    }).collect::<Vec<String>>();

    let dir_entries = entries.iter().filter_map(|entry| {
        if entry.is_dir() {
            Some(entry)
        } else {
            None
        }
    }).map(|entry| {
        entry.as_os_str().to_string_lossy().to_string()
    }).collect::<Vec<String>>();
    let child_pngs : Vec<String> = dir_entries.iter().map(find_png_paths).into_iter().flatten().collect();
    png_entries.into_iter().chain(child_pngs).collect()
}

fn main() -> Result<(), EncodingError> {
    let cwd = String::from(".");
    let pngs = find_png_paths(&cwd);
    let mut handles = vec![];
    for png in pngs {
        handles.push(thread::spawn(move || {
            compress_file(&png, &png)
        }));
    }
    let mut i = 0_f32;
    let len = handles.len() as f32;
    for handle in handles {
        let _ = handle.join();
        println!("{:06.2}%", (i / len) * 100.0);
        i = i + 1.0;
    }
    println!("{:06.2}%", 100.0);
    Ok(())
}
