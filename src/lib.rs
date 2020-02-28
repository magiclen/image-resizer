//! # Image Resizer
//! Resize or just shrink images and sharpen them appropriately.

extern crate clap;
extern crate image_convert;
extern crate num_cpus;
extern crate path_absolutize;
extern crate pathdiff;
extern crate scanner_rust;
extern crate starts_ends_with_caseless;
extern crate threadpool;
extern crate walkdir;

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use clap::{App, Arg};
use terminal_size::{terminal_size, Width};

use path_absolutize::*;

use image_convert::{
    identify, to_gif, to_jpg, to_pgm, to_png, to_webp, GIFConfig, ImageIdentify, ImageResource,
    JPGConfig, PGMConfig, PNGConfig, WEBPConfig,
};

use scanner_rust::Scanner;

use starts_ends_with_caseless::{StartsWithCaseless, StartsWithCaselessMultiple};

use walkdir::WalkDir;

use threadpool::ThreadPool;

// TODO -----Config START-----

const APP_NAME: &str = "Image Resizer";
const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
const CARGO_PKG_AUTHORS: &str = env!("CARGO_PKG_AUTHORS");

#[derive(Debug)]
pub struct Config {
    pub input: String,
    pub output: Option<String>,
    pub single_thread: bool,
    pub force: bool,
    pub allow_gif: bool,
    pub remain_profile: bool,
    pub side_maximum: u16,
    pub only_shrink: bool,
    pub sharpen: bool,
    pub quality: u8,
    pub force_to_chroma_quartered: bool,
}

impl Config {
    pub fn from_cli() -> Result<Config, String> {
        let arg0 = env::args().next().unwrap();
        let arg0 = Path::new(&arg0).file_stem().unwrap().to_str().unwrap();

        let examples = vec![
            "/path/to/image -m 1920                         # Make /path/to/image resized",
            "/path/to/folder -m 1920                        # Make images inside /path/to/folder and make resized",
            "/path/to/image -o /path/to/image2 -m 1920      # Make /path/to/image resized, and save it to /path/to/image2",
            "/path/to/folder -o /path/to/folder2 -m 1920    # Make images inside /path/to/folder resized, and save them to /path/to/folder2",
            "/path/to/folder -o /path/to/folder2 -f -m 1920 # Make images inside /path/to/folder resized, and save them to /path/to/folder2 without overwriting checks",
            "/path/to/folder --allow-gif -r -m 1920         # Make images inside /path/to/folder including GIF resized and also remain their profiles",
            "/path/to/image -m 1920 --shrink                # Make /path/to/image shrunk if it needs to be",
            "/path/to/image -m 1920 -q 75                   # Make /path/to/image resized with a quality of 75 if it uses lossy compression",
            "/path/to/image -m 1920 --4:2:0                 # Make /path/to/image resized and output using 4:2:0 (chroma quartered) subsampling to reduce the file size",
            "/path/to/image -m 1920 --no-sharpen            # Make /path/to/image resized without auto sharpening",
        ];

        let terminal_width = if let Some((Width(width), _)) = terminal_size() {
            width as usize
        } else {
            0
        };

        let matches = App::new(APP_NAME)
            .set_term_width(terminal_width)
            .version(CARGO_PKG_VERSION)
            .author(CARGO_PKG_AUTHORS)
            .about(format!("Resize or just shrink images and sharpen them appropriately.\n\nEXAMPLES:\n{}", examples.iter()
                .map(|e| format!("  {} {}\n", arg0, e))
                .collect::<Vec<String>>()
                .concat()
            ).as_str()
            )
            .arg(Arg::with_name("INPUT_PATH")
                .required(true)
                .help("Assigns an image or a directory for image resizing. It should be a path of a file or a directory")
                .takes_value(true)
            )
            .arg(Arg::with_name("OUTPUT_PATH")
                .required(false)
                .long("output")
                .short("o")
                .help("Assigns a destination of your generated files. It should be a path of a directory or a file depending on your input path")
                .takes_value(true)
            )
            .arg(Arg::with_name("SINGLE_THREAD")
                .long("single-thread")
                .short("s")
                .help("Uses only one thread")
            )
            .arg(Arg::with_name("FORCE")
                .long("force")
                .short("f")
                .help("Forces to overwrite files")
            )
            .arg(Arg::with_name("ALLOW_GIF")
                .long("allow-gif")
                .help("Allows to do GIF resizing")
            )
            .arg(Arg::with_name("REMAIN_PROFILE")
                .long("remain-profile")
                .short("r")
                .help("Remains the profiles of all images")
            )
            .arg(Arg::with_name("SIDE_MAXIMUM")
                .long("side-maximum")
                .visible_aliases(&["max"])
                .short("m")
                .takes_value(true)
                .required(true)
                .help("Sets the maximum pixels of each side of an image (Aspect ratio will be preserved)")
            )
            .arg(Arg::with_name("ONLY_SHRINK")
                .visible_aliases(&["shrink"])
                .long("only-shrink")
                .help("Only shrink images, not enlarge them")
            )
            .arg(Arg::with_name("QUALITY")
                .long("quality")
                .short("q")
                .takes_value(true)
                .default_value("92")
                .help("Sets the quality for lossy compression")
            )
            .arg(Arg::with_name("CHROMA_QUARTERED")
                .long("chroma-quartered")
                .visible_aliases(&["4:2:0"])
                .help("Uses 4:2:0 (chroma quartered) subsampling to reduce the file size if it is supported")
            )
            .arg(Arg::with_name("NO_SHARPEN")
                .long("no-sharpen")
                .help("Disables automatically sharpening")
            )
            .after_help("Enjoy it! https://magiclen.org")
            .get_matches();

        let input = matches.value_of("INPUT_PATH").unwrap().to_string();

        let output = matches.value_of("OUTPUT_PATH").map(|s| s.to_string());

        let single_thread = matches.is_present("SINGLE_THREAD");

        let force = matches.is_present("FORCE");

        let allow_gif = matches.is_present("ALLOW_GIF");

        let remain_profile = matches.is_present("REMAIN_PROFILE");

        let side_maximum = matches
            .value_of("SIDE_MAXIMUM")
            .unwrap()
            .parse::<u16>()
            .map_err(|_| String::from("You need to input a valid maximum for image sides."))?;

        if side_maximum == 0 {
            return Err(String::from("The maximum for image sides must be bigger than 0."));
        }

        let only_shrink = matches.is_present("ONLY_SHRINK");

        let sharpen = !matches.is_present("NO_SHARP");

        let quality = matches.value_of("QUALITY").unwrap().parse::<u8>().map_err(|_| {
            String::from("You need to input a valid quality value for lossy-compressed images.")
        })?;

        if quality > 100 {
            return Err(String::from("The range of quality is from 0 to 100."));
        }

        let force_to_chroma_quartered = matches.is_present("CHROMA_QUARTERED");

        Ok(Config {
            input,
            output,
            single_thread,
            force,
            allow_gif,
            remain_profile,
            side_maximum,
            only_shrink,
            sharpen,
            quality,
            force_to_chroma_quartered,
        })
    }
}

// TODO -----Config END-----

pub fn run(config: Config) -> Result<i32, String> {
    let (input_path, is_file) = match Path::new(config.input.as_str()).canonicalize() {
        Ok(path) => {
            let metadata = path.metadata().map_err(|err| err.to_string())?;

            let file_type = metadata.file_type();

            let is_file = file_type.is_file();

            if !path.is_dir() && !is_file {
                return Err(format!(
                    "`{}` is not an existing file or a directory.",
                    path.to_string_lossy()
                ));
            }

            (path, is_file)
        }
        Err(err) => {
            return Err(err.to_string());
        }
    };

    let output_path = match config.output.as_ref() {
        Some(output) => {
            let output_path = Path::new(output).absolutize().map_err(|err| err.to_string())?;

            if let Ok(metadata) = output_path.metadata() {
                let file_type = metadata.file_type();

                if file_type.is_file() {
                    if !is_file {
                        return Err(format!("`{}` is not a file.", output_path.to_string_lossy()));
                    }
                } else if file_type.is_dir() && is_file {
                    return Err(format!("`{}` is not a directory.", output_path.to_string_lossy()));
                }
            }

            Some(output_path)
        }
        None => None,
    };

    let sc: Arc<Mutex<Scanner<io::Stdin>>> =
        Arc::new(Mutex::new(Scanner::scan_stream(io::stdin())));
    let overwriting: Arc<Mutex<u8>> = Arc::new(Mutex::new(0));

    if is_file {
        resizing(
            config.allow_gif,
            config.remain_profile,
            config.force,
            config.side_maximum,
            config.only_shrink,
            config.sharpen,
            config.quality,
            config.force_to_chroma_quartered,
            &sc,
            &overwriting,
            &input_path,
            output_path.as_deref(),
        )?;
    } else {
        let mut image_paths = Vec::new();

        for entry in WalkDir::new(&input_path).into_iter().filter_map(|e| e.ok()) {
            let p = entry.path();

            if !p.is_file() {
                continue;
            }

            if let Some(extension) = p.extension() {
                if let Some(extension) = extension.to_str() {
                    let mut allow_extensions = vec!["jpg", "jpeg", "png", "webp", "ico", "pgm"];

                    if config.allow_gif {
                        allow_extensions.push("gif");
                    }

                    if extension.starts_with_caseless_ascii_multiple(&allow_extensions).is_some() {
                        image_paths.push(p.canonicalize().unwrap());
                    }
                }
            }
        }

        if config.single_thread {
            for image_path in image_paths {
                let output_path = match output_path.as_ref() {
                    Some(output_path) => {
                        let p = pathdiff::diff_paths(&image_path, &input_path).unwrap();

                        let output_path = output_path.join(&p);

                        Some(output_path)
                    }
                    None => None,
                };

                if let Err(err) = resizing(
                    config.allow_gif,
                    config.remain_profile,
                    config.force,
                    config.side_maximum,
                    config.only_shrink,
                    config.sharpen,
                    config.quality,
                    config.force_to_chroma_quartered,
                    &sc,
                    &overwriting,
                    image_path.as_path(),
                    output_path.as_deref(),
                ) {
                    eprintln!("{}", err);
                    io::stderr().flush().map_err(|err| err.to_string())?;
                }
            }
        } else {
            let cpus = num_cpus::get();

            let pool = ThreadPool::new(cpus * 2);

            for image_path in image_paths {
                let allow_gif = config.allow_gif;
                let remain_profile = config.remain_profile;
                let force = config.force;
                let side_maximum = config.side_maximum;
                let only_shrink = config.only_shrink;
                let sharpen = config.sharpen;
                let quality = config.quality;
                let force_to_chroma_quartered = config.force_to_chroma_quartered;
                let output_path = match output_path.as_ref() {
                    Some(output_path) => {
                        let p = pathdiff::diff_paths(&image_path, &input_path).unwrap();

                        let output_path = output_path.join(&p);

                        Some(output_path)
                    }
                    None => None,
                };

                let sc = sc.clone();
                let overwriting = overwriting.clone();

                pool.execute(move || {
                    if let Err(err) = resizing(
                        allow_gif,
                        remain_profile,
                        force,
                        side_maximum,
                        only_shrink,
                        sharpen,
                        quality,
                        force_to_chroma_quartered,
                        &sc,
                        &overwriting,
                        image_path.as_path(),
                        output_path.as_deref(),
                    ) {
                        eprintln!("{}", err);
                        io::stderr().flush().unwrap();
                    }
                });
            }

            pool.join();
        }
    }

    Ok(0)
}

#[allow(clippy::too_many_arguments)]
fn resizing(
    allow_gif: bool,
    remain_profile: bool,
    force: bool,
    side_maximum: u16,
    only_shrink: bool,
    sharpen: bool,
    quality: u8,
    force_to_chroma_quartered: bool,
    sc: &Arc<Mutex<Scanner<io::Stdin>>>,
    overwriting: &Arc<Mutex<u8>>,
    input_path: &Path,
    output_path: Option<&Path>,
) -> Result<(), String> {
    let mut output = None;

    let input_image_resource = ImageResource::from_path(&input_path);

    let input_identify: ImageIdentify =
        identify(&mut output, &input_image_resource).map_err(|err| err.to_string())?;

    match input_identify.format.as_str() {
        "JPEG" => {
            if let Some(output_path) =
                get_output_path(force, sc, overwriting, input_path, output_path)?
            {
                let mut config = JPGConfig::new();

                config.remain_profile = remain_profile;
                config.width = side_maximum;
                config.height = side_maximum;
                config.shrink_only = only_shrink;

                if !sharpen {
                    config.sharpen = 0f64;
                }

                config.quality = quality;
                config.force_to_chroma_quartered = force_to_chroma_quartered;

                let mut output = ImageResource::from_path(output_path);

                to_jpg(&mut output, &input_image_resource, &config)
                    .map_err(|err| err.to_string())?;

                print_resized_message(output_path)?;
            }
        }
        "PNG" => {
            if let Some(output_path) =
                get_output_path(force, sc, overwriting, input_path, output_path)?
            {
                let mut config = PNGConfig::new();

                config.remain_profile = remain_profile;
                config.width = side_maximum;
                config.height = side_maximum;
                config.shrink_only = only_shrink;

                if !sharpen {
                    config.sharpen = 0f64;
                }

                let mut output = ImageResource::from_path(output_path);

                to_png(&mut output, &input_image_resource, &config)
                    .map_err(|err| err.to_string())?;

                print_resized_message(output_path)?;
            }
        }
        "WEBP" => {
            if let Some(output_path) =
                get_output_path(force, sc, overwriting, input_path, output_path)?
            {
                let mut config = WEBPConfig::new();

                config.remain_profile = remain_profile;
                config.width = side_maximum;
                config.height = side_maximum;
                config.shrink_only = only_shrink;

                if !sharpen {
                    config.sharpen = 0f64;
                }

                config.quality = quality;

                let mut output = ImageResource::from_path(output_path);

                to_webp(&mut output, &input_image_resource, &config)
                    .map_err(|err| err.to_string())?;

                print_resized_message(output_path)?;
            }
        }
        "PGM" => {
            if let Some(output_path) =
                get_output_path(force, sc, overwriting, input_path, output_path)?
            {
                let mut config = PGMConfig::new();

                config.remain_profile = remain_profile;
                config.width = side_maximum;
                config.height = side_maximum;
                config.shrink_only = only_shrink;

                if !sharpen {
                    config.sharpen = 0f64;
                }

                let mut output = ImageResource::from_path(output_path);

                to_pgm(&mut output, &input_image_resource, &config)
                    .map_err(|err| err.to_string())?;

                print_resized_message(output_path)?;
            }
        }
        "GIF" => {
            if allow_gif {
                if let Some(output_path) =
                    get_output_path(force, sc, overwriting, input_path, output_path)?
                {
                    let mut config = GIFConfig::new();

                    config.remain_profile = remain_profile;
                    config.width = side_maximum;
                    config.height = side_maximum;
                    config.shrink_only = only_shrink;

                    if !sharpen {
                        config.sharpen = 0f64;
                    }

                    let mut output = ImageResource::from_path(output_path);

                    to_gif(&mut output, &input_image_resource, &config)
                        .map_err(|err| err.to_string())?;

                    print_resized_message(output_path)?;
                }
            }
        }
        _ => (),
    }

    Ok(())
}

fn get_output_path<'a>(
    force: bool,
    sc: &Arc<Mutex<Scanner<io::Stdin>>>,
    overwriting: &Arc<Mutex<u8>>,
    input_path: &'a Path,
    output_path: Option<&'a Path>,
) -> Result<Option<&'a Path>, String> {
    match output_path {
        Some(output_path) => {
            if output_path.exists() {
                if !force {
                    let mutex_guard = overwriting.lock().unwrap();

                    let output_path_string = output_path.to_string_lossy();

                    let allow_overwrite = loop {
                        print!(
                            "`{}` exists, do you want to overwrite it? [y/n] ",
                            output_path_string
                        );
                        io::stdout().flush().map_err(|_| "Cannot flush stdout.".to_string())?;

                        let token = sc
                            .lock()
                            .unwrap()
                            .next()
                            .map_err(|_| "Cannot read from stdin.".to_string())?
                            .ok_or_else(|| "Read EOF.".to_string())?;

                        if token.starts_with_caseless_ascii("y") {
                            break true;
                        } else if token.starts_with_caseless_ascii("n") {
                            break false;
                        }
                    };

                    drop(mutex_guard);

                    if !allow_overwrite {
                        return Ok(None);
                    }
                }
            } else {
                fs::create_dir_all(output_path.parent().unwrap()).map_err(|err| err.to_string())?;
            }

            Ok(Some(output_path))
        }
        None => Ok(Some(input_path)),
    }
}

#[inline]
fn print_resized_message(output_path: &Path) -> Result<(), String> {
    println!("`{}` has been resized.", output_path.to_string_lossy());
    io::stdout().flush().map_err(|_| "Cannot flush stdout.".to_string())
}
