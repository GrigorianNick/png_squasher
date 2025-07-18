use std::{env::set_current_dir, fs, os::windows::fs::MetadataExt, path::PathBuf, thread};

use clap::Parser;
use image::{codecs::png::PngEncoder, imageops::{resize, FilterType::{self, Gaussian}}, DynamicImage, GenericImage, GenericImageView, ImageReader, Rgb, RgbImage};
use tempfile::NamedTempFile;

#[derive(clap::ValueEnum, Copy, Clone, Default, Debug)]
enum Filter {
    #[default]
    Gaussian,
    Lanczos,
    CatmullRom,
    NearestNeighbor,
    LinearTriangle,
}

fn convert_filter(filter: Filter) -> FilterType {
    match filter {
        Filter::Gaussian => FilterType::Gaussian,
        Filter::Lanczos => FilterType::Lanczos3,
        Filter::CatmullRom => FilterType::CatmullRom,
        Filter::NearestNeighbor => FilterType::Nearest,
        Filter::LinearTriangle => FilterType::Triangle
    }
}

/// A helper util that will search for pngs in the current directory tree and then compress them
#[derive(Parser, Debug)]
struct Args {
    /// Maximum number of pixels pngs are allowed to have on the x axis. Larger images will be scaled down
    #[arg(short, long)]
    x_max: Option<u32>,

    /// Maximum number of pixels pngs are allowed to have on the y axis. Larger images will be scaled down
    #[arg(short, long)]
    y_max: Option<u32>,

    /// Directory to start the recursive png search
    #[arg(short, long)]
    dir: Option<String>,

    #[arg(short, long, default_value_t, value_enum)]
    filter: Filter
}

fn load_and_preprocess(file_path: &String) -> Result<Vec<DynamicImage>, Box<dyn std::error::Error>> {
    let loaded_image = ImageReader::open(file_path)?.decode()?;
    if !loaded_image.color().has_alpha() {
        return Ok(vec![loaded_image]);
    }
    
    if loaded_image.pixels().into_iter().any(|p| p.2.0[3] < 254) {
        return Ok(vec![loaded_image]);
    } else {
        let mut stripped_image = RgbImage::new(loaded_image.width(), loaded_image.height());
        for pixel in loaded_image.pixels() {
            stripped_image.put_pixel(pixel.0, pixel.1, Rgb([pixel.2.0[0], pixel.2.0[1], pixel.2.0[2]]));
        }
        Ok(vec![loaded_image.clone(), stripped_image.into()])
    }
}

fn compress_image(loaded_image: DynamicImage, outfile_name: &String, nwidth: u32, nheight: u32, filter: Filter) -> Result<(), Box<dyn std::error::Error>> {

        let temp_path = NamedTempFile::new()?;
        let smaller_image = resize(&loaded_image, nwidth, nheight, convert_filter(filter));
        let png_encoder = PngEncoder::new_with_quality(&temp_path, image::codecs::png::CompressionType::Best, image::codecs::png::FilterType::Adaptive);
        smaller_image.write_with_encoder(png_encoder)?;

        if let Ok(true) = fs::exists(outfile_name) {
            let target_metadata = fs::metadata(outfile_name)?;
            let temp_metadata = fs::metadata(temp_path.path())?;
            if target_metadata.file_size() < temp_metadata.file_size() {
                return Ok(());
            }
            let mut perms = std::fs::metadata(outfile_name)?.permissions();
            if perms.readonly() {
                perms.set_readonly(false);
                std::fs::set_permissions(outfile_name, perms)?;
            }
        }
        Ok(std::fs::rename(temp_path.path(), outfile_name)?)
}

fn compress_images(infile_name: &String, outfile_name: &String, max_width: Option<u32>, max_height: Option<u32>, filter: Filter) -> Result<(), Box<dyn std::error::Error>> {
    let loaded_images = load_and_preprocess(infile_name)?;
    for loaded_image in loaded_images {
        let (nwidth, nheight) = match (max_width, max_height) {
            (None, None) => (loaded_image.width(), loaded_image.height()),
            (None, Some(max_h)) => ((loaded_image.width() as f32 * (max_h as f32 / loaded_image.height() as f32)) as u32, max_h),
            (Some(max_w), None) => (max_w, ((loaded_image.height() as f32 * (max_w as f32 / loaded_image.width() as f32)) as u32)),
            (Some(max_w), Some(max_h)) => {
                let w_ratio = (max_w as f32 / loaded_image.width() as f32).min(1.0);
                let h_ratio = (max_h as f32 / loaded_image.height() as f32).min(1.0);
                ((loaded_image.width() as f32 * w_ratio.min(h_ratio)) as u32,
                (loaded_image.height() as f32 * w_ratio.min(h_ratio)) as u32)
            },
        };

        compress_image(loaded_image, outfile_name, nwidth, nheight, filter)?;
    }

    Ok(())
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

fn main() -> Result<(), Box<dyn std::error::Error>> {

    let args = Args::parse();

    if let Some(path) = args.dir {
        set_current_dir(path)?;
    }

    let cwd = String::from(".");
    let pngs = find_png_paths(&cwd);
    let mut handles = vec![];
    for png in pngs {
        handles.push(thread::spawn(move || {
            if let Err(e) =  compress_images(&png, &png, args.x_max, args.y_max, args.filter) {
                println!("{}:{}", png, e);
            }
        }));
    }
    let mut i = 0_f32;
    let len = handles.len() as f32;
    for handle in handles {
        handle.is_finished();
        let _ = handle.join();
        println!("{:06.2}%", (i / len) * 100.0);
        i = i + 1.0;
    }
    println!("{:06.2}%", 100.0);
    Ok(())
}