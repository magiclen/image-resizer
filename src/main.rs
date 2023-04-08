use std::{
    error::Error,
    fs,
    io::{self, Write},
    path::Path,
    sync::{Arc, Mutex},
};

use clap::{Arg, Command};
use concat_with::concat_line;
use path_absolutize::Absolutize;
use scanner_rust::{generic_array::typenum::U8, Scanner};
use str_utils::{EqIgnoreAsciiCaseMultiple, StartsWithIgnoreAsciiCase};
use terminal_size::terminal_size;
use threadpool::ThreadPool;
use walkdir::WalkDir;

const APP_NAME: &str = "Image Resizer";
const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
const CARGO_PKG_AUTHORS: &str = env!("CARGO_PKG_AUTHORS");

fn main() -> Result<(), Box<dyn Error>> {
    let matches = Command::new(APP_NAME)
        .term_width(terminal_size().map(|(width, _)| width.0 as usize).unwrap_or(0))
        .version(CARGO_PKG_VERSION)
        .author(CARGO_PKG_AUTHORS)
        .about(concat!("Resize or just shrink images and sharpen them appropriately.\n\nEXAMPLES:\n", concat_line!(prefix "image-resizer ",
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
                "/path/to/image -m 1920 --ppi 150               # Make /path/to/image resized, and set their PPI to 150",
            )))
        .arg(Arg::new("INPUT_PATH")
            .required(true)
            .help("Assign an image or a directory for image resizing. It should be a path of a file or a directory")
            .takes_value(true)
        )
        .arg(Arg::new("OUTPUT_PATH")
            .required(false)
            .long("output")
            .short('o')
            .help("Assign a destination of your generated files. It should be a path of a directory or a file depending on your input path")
            .takes_value(true)
        )
        .arg(Arg::new("SINGLE_THREAD")
            .long("single-thread")
            .short('s')
            .help("Use only one thread")
        )
        .arg(Arg::new("FORCE")
            .long("force")
            .short('f')
            .help("Force to overwrite files")
        )
        .arg(Arg::new("ALLOW_GIF")
            .long("allow-gif")
            .help("Allow to do GIF resizing")
        )
        .arg(Arg::new("REMAIN_PROFILE")
            .long("remain-profile")
            .short('r')
            .help("Remain the profiles of all images")
        )
        .arg(Arg::new("SIDE_MAXIMUM")
            .long("side-maximum")
            .visible_aliases(&["max"])
            .short('m')
            .takes_value(true)
            .required(true)
            .help("Set the maximum pixels of each side of an image (Aspect ratio will be preserved)")
        )
        .arg(Arg::new("ONLY_SHRINK")
            .visible_aliases(&["shrink"])
            .long("only-shrink")
            .help("Only shrink images, not enlarge them")
        )
        .arg(Arg::new("NO_SHARPEN")
            .long("no-sharpen")
            .help("Disable automatically sharpening")
        )
        .arg(Arg::new("QUALITY")
            .long("quality")
            .short('q')
            .takes_value(true)
            .default_value("92")
            .help("Set the quality for lossy compression")
        )
        .arg(Arg::new("PPI")
            .long("ppi")
            .takes_value(true)
            .help("Set pixels per inch (ppi)")
        )
        .arg(Arg::new("CHROMA_QUARTERED")
            .long("chroma-quartered")
            .visible_aliases(&["4:2:0"])
            .help("Use 4:2:0 (chroma quartered) subsampling to reduce the file size if it is supported")
        )
        .after_help("Enjoy it! https://magiclen.org")
        .get_matches();

    let input = matches.value_of("INPUT_PATH").unwrap();
    let output = matches.value_of("OUTPUT_PATH");

    let single_thread = matches.is_present("SINGLE_THREAD");
    let force = matches.is_present("FORCE");
    let allow_gif = matches.is_present("ALLOW_GIF");
    let remain_profile = matches.is_present("REMAIN_PROFILE");
    let only_shrink = matches.is_present("ONLY_SHRINK");
    let sharpen = !matches.is_present("NO_SHARP");
    let force_to_chroma_quartered = matches.is_present("CHROMA_QUARTERED");

    let side_maximum = matches
        .value_of("SIDE_MAXIMUM")
        .unwrap()
        .parse::<u16>()
        .map_err(|_| "You need to input a valid maximum for image sides.")?;

    let quality = matches
        .value_of("QUALITY")
        .unwrap()
        .parse::<u8>()
        .map_err(|_| "You need to input a valid quality value for lossy-compressed images.")?;

    if quality > 100 {
        return Err("The range of quality is from 0 to 100.".into());
    }

    let ppi = match matches.value_of("PPI") {
        Some(ppi) => {
            let ppi = ppi.parse::<f64>().map_err(|_| {
                "You need to input a valid quality value for pixels per inch (ppi)."
            })?;

            if ppi <= 0f64 {
                return Err("The ppi must be bigger than 0.".into());
            }

            Some(ppi)
        },
        None => None,
    };

    let input_path = Path::new(input);

    let is_dir = input_path.metadata()?.is_dir();

    let output_path = match output {
        Some(output) => {
            let output_path = Path::new(output);

            if is_dir {
                match output_path.metadata() {
                    Ok(metadata) => {
                        if metadata.is_dir() {
                            Some(output_path)
                        } else {
                            return Err(format!(
                                "`{}` is not a directory.",
                                output_path.absolutize()?.to_string_lossy()
                            )
                            .into());
                        }
                    },
                    Err(_) => {
                        fs::create_dir_all(output_path)?;

                        Some(output_path)
                    },
                }
            } else if output_path.is_dir() {
                return Err(format!(
                    "`{}` is not a file.",
                    output_path.absolutize()?.to_string_lossy()
                )
                .into());
            } else {
                Some(output_path)
            }
        },
        None => None,
    };

    let sc: Arc<Mutex<Scanner<io::Stdin, U8>>> = Arc::new(Mutex::new(Scanner::new2(io::stdin())));
    let overwriting: Arc<Mutex<u8>> = Arc::new(Mutex::new(0));

    if is_dir {
        let mut image_paths = Vec::new();

        for dir_entry in WalkDir::new(input_path).into_iter().filter_map(|e| e.ok()) {
            if !dir_entry.metadata()?.is_file() {
                continue;
            }

            let p = dir_entry.into_path();

            if let Some(extension) = p.extension() {
                if let Some(extension) = extension.to_str() {
                    let mut allow_extensions = vec!["jpg", "jpeg", "png"];

                    if allow_gif {
                        allow_extensions.push("gif");
                    }

                    if extension
                        .eq_ignore_ascii_case_with_lowercase_multiple(&allow_extensions)
                        .is_some()
                    {
                        image_paths.push(p);
                    }
                }
            }
        }

        if single_thread {
            for image_path in image_paths {
                let output_path = match output_path.as_ref() {
                    Some(output_path) => {
                        let p = pathdiff::diff_paths(&image_path, input_path).unwrap();

                        let output_path = output_path.join(p);

                        Some(output_path)
                    },
                    None => None,
                };

                if let Err(err) = resizing(
                    allow_gif,
                    remain_profile,
                    force,
                    side_maximum,
                    only_shrink,
                    sharpen,
                    quality,
                    ppi,
                    force_to_chroma_quartered,
                    &sc,
                    &overwriting,
                    image_path.as_path(),
                    output_path.as_deref(),
                ) {
                    eprintln!("{}", err);
                    io::stderr().flush()?;
                }
            }
        } else {
            let cpus = num_cpus::get();

            let pool = ThreadPool::new(cpus * 2);

            for image_path in image_paths {
                let sc = sc.clone();
                let overwriting = overwriting.clone();
                let output_path = match output_path.as_ref() {
                    Some(output_path) => {
                        let p = pathdiff::diff_paths(&image_path, input_path).unwrap();

                        let output_path = output_path.join(p);

                        Some(output_path)
                    },
                    None => None,
                };

                pool.execute(move || {
                    if let Err(err) = resizing(
                        allow_gif,
                        remain_profile,
                        force,
                        side_maximum,
                        only_shrink,
                        sharpen,
                        quality,
                        ppi,
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
    } else {
        resizing(
            allow_gif,
            remain_profile,
            force,
            side_maximum,
            only_shrink,
            sharpen,
            quality,
            ppi,
            force_to_chroma_quartered,
            &sc,
            &overwriting,
            input_path,
            output_path,
        )?;
    }

    Ok(())
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
    ppi: Option<f64>,
    force_to_chroma_quartered: bool,
    sc: &Arc<Mutex<Scanner<io::Stdin, U8>>>,
    overwriting: &Arc<Mutex<u8>>,
    input_path: &Path,
    output_path: Option<&Path>,
) -> Result<(), Box<dyn Error>> {
    let input_image_resource = image_convert::ImageResource::from_path(input_path);

    let input_identify = image_convert::identify_ping(&input_image_resource)?;

    match input_identify.format.as_str() {
        "JPEG" => {
            if let Some(output_path) =
                get_output_path(force, sc, overwriting, input_path, output_path)?
            {
                let mut config = image_convert::JPGConfig::new();

                config.remain_profile = remain_profile;
                config.width = side_maximum;
                config.height = side_maximum;
                config.shrink_only = only_shrink;

                if !sharpen {
                    config.sharpen = 0f64;
                }

                config.quality = quality;

                if let Some(ppi) = ppi {
                    config.ppi = Some((ppi, ppi));
                }

                config.force_to_chroma_quartered = force_to_chroma_quartered;

                let mut output = image_convert::ImageResource::from_path(output_path);

                image_convert::to_jpg(&mut output, &input_image_resource, &config)?;

                print_resized_message(output_path)?;
            }
        },
        "PNG" => {
            if let Some(output_path) =
                get_output_path(force, sc, overwriting, input_path, output_path)?
            {
                let mut config = image_convert::PNGConfig::new();

                config.remain_profile = remain_profile;
                config.width = side_maximum;
                config.height = side_maximum;
                config.shrink_only = only_shrink;

                if !sharpen {
                    config.sharpen = 0f64;
                }

                if let Some(ppi) = ppi {
                    config.ppi = Some((ppi, ppi));
                }

                let mut output = image_convert::ImageResource::from_path(output_path);

                image_convert::to_png(&mut output, &input_image_resource, &config)?;

                print_resized_message(output_path)?;
            }
        },
        "TIFF" => {
            if let Some(output_path) =
                get_output_path(force, sc, overwriting, input_path, output_path)?
            {
                let mut config = image_convert::TIFFConfig::new();

                config.remain_profile = remain_profile;
                config.width = side_maximum;
                config.height = side_maximum;
                config.shrink_only = only_shrink;

                if !sharpen {
                    config.sharpen = 0f64;
                }

                if let Some(ppi) = ppi {
                    config.ppi = Some((ppi, ppi));
                }

                let mut output = image_convert::ImageResource::from_path(output_path);

                image_convert::to_tiff(&mut output, &input_image_resource, &config)?;

                print_resized_message(output_path)?;
            }
        },
        "WEBP" => {
            if let Some(output_path) =
                get_output_path(force, sc, overwriting, input_path, output_path)?
            {
                let mut config = image_convert::WEBPConfig::new();

                config.remain_profile = remain_profile;
                config.width = side_maximum;
                config.height = side_maximum;
                config.shrink_only = only_shrink;

                if !sharpen {
                    config.sharpen = 0f64;
                }

                config.quality = quality;

                let mut output = image_convert::ImageResource::from_path(output_path);

                image_convert::to_webp(&mut output, &input_image_resource, &config)?;

                print_resized_message(output_path)?;
            }
        },
        "PGM" => {
            if let Some(output_path) =
                get_output_path(force, sc, overwriting, input_path, output_path)?
            {
                let mut config = image_convert::PGMConfig::new();

                config.remain_profile = remain_profile;
                config.width = side_maximum;
                config.height = side_maximum;
                config.shrink_only = only_shrink;

                if !sharpen {
                    config.sharpen = 0f64;
                }

                let mut output = image_convert::ImageResource::from_path(output_path);

                image_convert::to_pgm(&mut output, &input_image_resource, &config)?;

                print_resized_message(output_path)?;
            }
        },
        "GIF" => {
            if allow_gif {
                if let Some(output_path) =
                    get_output_path(force, sc, overwriting, input_path, output_path)?
                {
                    let mut config = image_convert::GIFConfig::new();

                    config.remain_profile = remain_profile;
                    config.width = side_maximum;
                    config.height = side_maximum;
                    config.shrink_only = only_shrink;

                    if !sharpen {
                        config.sharpen = 0f64;
                    }

                    let mut output = image_convert::ImageResource::from_path(output_path);

                    image_convert::to_gif(&mut output, &input_image_resource, &config)?;

                    print_resized_message(output_path)?;
                }
            }
        },
        _ => (),
    }

    Ok(())
}

fn get_output_path<'a>(
    force: bool,
    sc: &Arc<Mutex<Scanner<io::Stdin, U8>>>,
    overwriting: &Arc<Mutex<u8>>,
    input_path: &'a Path,
    output_path: Option<&'a Path>,
) -> Result<Option<&'a Path>, Box<dyn Error>> {
    match output_path {
        Some(output_path) => {
            if output_path.exists() {
                if !force {
                    let mutex_guard = overwriting.lock().unwrap();

                    let allow_overwrite = loop {
                        print!(
                            "`{}` exists, do you want to overwrite it? [y/n] ",
                            output_path.absolutize()?.to_string_lossy()
                        );
                        io::stdout().flush()?;

                        let token = {
                            let s = sc.lock().unwrap().next()?;

                            s.ok_or_else(|| "Read EOF.".to_string())?
                        };

                        if token.starts_with_ignore_ascii_case_with_lowercase("y") {
                            break true;
                        } else if token.starts_with_ignore_ascii_case_with_lowercase("n") {
                            break false;
                        }
                    };

                    drop(mutex_guard);

                    if !allow_overwrite {
                        return Ok(None);
                    }
                }
            } else {
                fs::create_dir_all(output_path.parent().unwrap())?;
            }

            Ok(Some(output_path))
        },
        None => Ok(Some(input_path)),
    }
}

#[inline]
fn print_resized_message(output_path: &Path) -> Result<(), io::Error> {
    println!("`{}` has been resized.", output_path.absolutize()?.to_string_lossy());
    io::stdout().flush()
}
