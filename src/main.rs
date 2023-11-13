mod cli;

use std::{
    fs, io,
    io::Write,
    path::Path,
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, Context};
use cli::*;
use scanner_rust::{generic_array::typenum::U8, Scanner};
use str_utils::EqIgnoreAsciiCaseMultiple;
use threadpool::ThreadPool;
use walkdir::WalkDir;

fn main() -> anyhow::Result<()> {
    let args = get_args();

    let is_dir =
        args.input_path.metadata().with_context(|| anyhow!("{:?}", args.input_path))?.is_dir();

    if let Some(output_path) = args.output_path.as_deref() {
        if is_dir {
            match output_path.metadata() {
                Ok(metadata) => {
                    if !metadata.is_dir() {
                        return Err(anyhow!("{output_path:?} is not a directory.",));
                    }
                },
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    fs::create_dir_all(output_path)
                        .with_context(|| anyhow!("{:?}", output_path))?;
                },
                Err(error) => {
                    return Err(error).with_context(|| anyhow!("{:?}", output_path));
                },
            }
        } else if output_path.is_dir() {
            return Err(anyhow!("{output_path:?} is a directory."));
        }
    }

    let sc: Arc<Mutex<Scanner<io::Stdin, U8>>> = Arc::new(Mutex::new(Scanner::new2(io::stdin())));
    let overwriting: Arc<Mutex<u8>> = Arc::new(Mutex::new(0));

    if is_dir {
        let mut image_paths = Vec::new();

        for dir_entry in WalkDir::new(args.input_path.as_path()).into_iter().filter_map(|e| e.ok())
        {
            if !dir_entry.metadata()?.is_file() {
                continue;
            }

            let p = dir_entry.into_path();

            if let Some(extension) = p.extension() {
                if let Some(extension) = extension.to_str() {
                    let mut allow_extensions = vec!["jpg", "jpeg", "png"];

                    if args.allow_gif {
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

        if args.single_thread {
            for image_path in image_paths {
                let output_path = match args.output_path.as_ref() {
                    Some(output_path) => {
                        let p =
                            pathdiff::diff_paths(&image_path, args.input_path.as_path()).unwrap();

                        let output_path = output_path.join(p);

                        Some(output_path)
                    },
                    None => None,
                };

                resizing(
                    args.allow_gif,
                    args.remain_profile,
                    args.force,
                    args.side_maximum,
                    args.only_shrink,
                    !args.no_sharpen,
                    args.quality,
                    args.ppi,
                    args.chroma_quartered,
                    &sc,
                    &overwriting,
                    image_path.as_path(),
                    output_path.as_deref(),
                )?;
            }
        } else {
            let cpus = num_cpus::get();

            let pool = ThreadPool::new(cpus * 2);

            for image_path in image_paths {
                let sc = sc.clone();
                let overwriting = overwriting.clone();
                let output_path = match args.output_path.as_ref() {
                    Some(output_path) => {
                        let p =
                            pathdiff::diff_paths(&image_path, args.input_path.as_path()).unwrap();

                        let output_path = output_path.join(p);

                        Some(output_path)
                    },
                    None => None,
                };

                pool.execute(move || {
                    if let Err(error) = resizing(
                        args.allow_gif,
                        args.remain_profile,
                        args.force,
                        args.side_maximum,
                        args.only_shrink,
                        !args.no_sharpen,
                        args.quality,
                        args.ppi,
                        args.chroma_quartered,
                        &sc,
                        &overwriting,
                        image_path.as_path(),
                        output_path.as_deref(),
                    ) {
                        eprintln!("{error:?}");
                        io::stderr().flush().unwrap();
                    }
                });
            }

            pool.join();
        }
    } else {
        resizing(
            args.allow_gif,
            args.remain_profile,
            args.force,
            args.side_maximum,
            args.only_shrink,
            !args.no_sharpen,
            args.quality,
            args.ppi,
            args.chroma_quartered,
            &sc,
            &overwriting,
            args.input_path,
            args.output_path,
        )?;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn resizing<IP: AsRef<Path>, OP: AsRef<Path>>(
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
    input_path: IP,
    output_path: Option<OP>,
) -> anyhow::Result<()> {
    let input_path = input_path.as_ref();
    let output_path = output_path.as_ref().map(|p| p.as_ref());

    let input_image_resource = image_convert::ImageResource::from_path(input_path);

    let input_identify = image_convert::identify_ping(&input_image_resource)
        .with_context(|| anyhow!("{input_path:?}"))?;

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

                image_convert::to_jpg(&mut output, &input_image_resource, &config)
                    .with_context(|| anyhow!("to_jpg {output_path:?}"))?;

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

                image_convert::to_png(&mut output, &input_image_resource, &config)
                    .with_context(|| anyhow!("to_png {output_path:?}"))?;

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

                image_convert::to_tiff(&mut output, &input_image_resource, &config)
                    .with_context(|| anyhow!("to_tiff {output_path:?}"))?;

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

                image_convert::to_webp(&mut output, &input_image_resource, &config)
                    .with_context(|| anyhow!("to_webp {output_path:?}"))?;

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

                image_convert::to_pgm(&mut output, &input_image_resource, &config)
                    .with_context(|| anyhow!("to_pgm {output_path:?}"))?;

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

                    image_convert::to_gif(&mut output, &input_image_resource, &config)
                        .with_context(|| anyhow!("to_gif {output_path:?}"))?;

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
) -> anyhow::Result<Option<&'a Path>> {
    match output_path {
        Some(output_path) => {
            if output_path.exists() {
                if !force {
                    let mutex_guard = overwriting.lock().unwrap();

                    loop {
                        print!("{output_path:?} exists, do you want to overwrite it? [Y/N] ",);
                        io::stdout().flush().with_context(|| anyhow!("stdout"))?;

                        match sc.lock().unwrap().next_line().with_context(|| anyhow!("stdout"))? {
                            Some(token) => match token.to_ascii_uppercase().as_str() {
                                "Y" => {
                                    break;
                                },
                                "N" => {
                                    return Ok(None);
                                },
                                _ => {
                                    continue;
                                },
                            },
                            None => {
                                return Ok(None);
                            },
                        }
                    }

                    drop(mutex_guard);
                }
            } else {
                let dir_path = output_path.parent().unwrap();

                fs::create_dir_all(dir_path).with_context(|| anyhow!("{dir_path:?}"))?;
            }

            Ok(Some(output_path))
        },
        None => Ok(Some(input_path)),
    }
}

#[inline]
fn print_resized_message<P: AsRef<Path>>(path: P) -> anyhow::Result<()> {
    println!("{:?} has been resized.", path.as_ref().canonicalize().unwrap());
    io::stdout().flush()?;

    Ok(())
}
