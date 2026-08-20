mod cli;

use std::{
    fmt, fs, io,
    io::Write,
    path::{Path, PathBuf},
    process,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
};

use anyhow::{Context, anyhow};
use cli::*;
use threadpool::ThreadPool;
use walkdir::WalkDir;

const ALLOW_EXTENSIONS: [&str; 7] = ["jpg", "jpeg", "png", "webp", "tif", "tiff", "pgm"];
const ALLOW_EXTENSIONS_WITH_GIF: [&str; 8] =
    ["jpg", "jpeg", "png", "webp", "tif", "tiff", "pgm", "gif"];

/// The options which decide how an image is resized.
#[derive(Debug, Clone, Copy)]
struct Flags {
    allow_gif:        bool,
    remain_metadata:  bool,
    force:            bool,
    side_maximum:     u32,
    only_shrink:      bool,
    sharpen:          bool,
    quality:          Option<u8>,
    ppi:              Option<f64>,
    chroma_quartered: bool,
}

fn report_error(console_lock: &Mutex<()>, error: &anyhow::Error) {
    let _console_lock = console_lock.lock().unwrap();
    let mut stderr = io::stderr().lock();

    let _ = writeln!(stderr, "{error:?}");
    let _ = stderr.flush();
}

/// Prints a message to stdout. A closed pipe must not take the whole program down, so a failed write is dropped just like the ones to stderr.
fn report_message(console_lock: &Mutex<()>, message: fmt::Arguments<'_>) {
    let _console_lock = console_lock.lock().unwrap();
    let mut stdout = io::stdout().lock();

    let _ = writeln!(stdout, "{message}");
    let _ = stdout.flush();
}

/// Keeps ImageMagick from spreading a single operation over every core, because in this mode the thread pool is what parallelizes the work.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn limit_magick_threads() -> anyhow::Result<()> {
    use image_convert::magick_rust::{MagickWand, ResourceType};

    MagickWand::set_resource_limit(ResourceType::Thread, 1)
        .with_context(|| anyhow!("ImageMagick thread limit"))
}

/// `set_resource_limit` is not available on this platform, so ImageMagick keeps deciding its thread count on its own.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn limit_magick_threads() -> anyhow::Result<()> {
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let CLIArgs {
        mut input_path,
        output_path,
        single_thread,
        force,
        allow_gif,
        remain_metadata,
        side_maximum,
        only_shrink,
        no_sharpen,
        quality,
        ppi,
        chroma_quartered,
    } = get_args();

    let flags = Flags {
        allow_gif,
        remain_metadata,
        force,
        side_maximum,
        only_shrink,
        sharpen: !no_sharpen,
        quality,
        ppi,
        chroma_quartered,
    };

    // A symlink named on the command line is an alias for the real image, and renaming over it would replace the link itself instead of updating what it points at.
    if input_path
        .symlink_metadata()
        .with_context(|| anyhow!("{input_path:?}"))?
        .file_type()
        .is_symlink()
    {
        input_path = input_path.canonicalize().with_context(|| anyhow!("{input_path:?}"))?;
    }

    let is_dir = input_path.metadata().with_context(|| anyhow!("{input_path:?}"))?.is_dir();

    if let Some(output_path) = output_path.as_deref() {
        if is_dir {
            match output_path.metadata() {
                Ok(metadata) => {
                    if !metadata.is_dir() {
                        return Err(anyhow!("{output_path:?} is not a directory."));
                    }
                },
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    fs::create_dir_all(output_path).with_context(|| anyhow!("{output_path:?}"))?;
                },
                Err(error) => {
                    return Err(error).with_context(|| anyhow!("{output_path:?}"));
                },
            }
        } else if output_path.is_dir() {
            return Err(anyhow!("{output_path:?} is a directory."));
        }
    }

    // ImageMagick has to be initialized once before any wand is used. Doing it here keeps it on the main thread and lets the thread limit below apply to a ready environment.
    image_convert::start_call_once();

    let console_lock: Arc<Mutex<()>> = Arc::new(Mutex::new(()));
    let error_count = Arc::new(AtomicUsize::new(0));

    if is_dir {
        let mut image_paths = Vec::new();

        for dir_entry in WalkDir::new(input_path.as_path()) {
            let dir_entry = match dir_entry {
                Ok(dir_entry) => dir_entry,
                Err(error) => {
                    // Dropping the entry silently would hide a whole unreadable subtree and still report success.
                    report_error(&console_lock, &anyhow::Error::new(error));
                    error_count.fetch_add(1, Ordering::Relaxed);

                    continue;
                },
            };

            // `file_type` reuses what the directory listing already reported, so it costs no syscall.
            // A symlink either duplicates a file which is walked on its own or points outside the requested tree, and neither is this program's business.
            if !dir_entry.file_type().is_file() {
                continue;
            }

            let p = dir_entry.into_path();

            if let Some(extension) = p.extension().and_then(|extension| extension.to_str()) {
                if is_allowed_extension(extension, allow_gif) {
                    image_paths.push(p);
                }
            }
        }

        if single_thread {
            for image_path in image_paths {
                resize_entry(
                    flags,
                    &console_lock,
                    &error_count,
                    input_path.as_path(),
                    output_path.as_deref(),
                    image_path.as_path(),
                );
            }
        } else {
            let cpus = thread::available_parallelism().map(|cpus| cpus.get()).unwrap_or(1);

            limit_magick_threads()?;

            // ImageMagick parallelizes each operation internally, so one worker per core keeps the throughput while holding far fewer decoded images at once.
            let pool = ThreadPool::new(cpus);

            // Every job reads the same two roots, so they are shared instead of being copied into each of them.
            let input_root: Arc<Path> = Arc::from(input_path.as_path());
            let output_root: Option<Arc<Path>> = output_path.as_deref().map(Arc::from);

            for image_path in image_paths {
                let console_lock = console_lock.clone();
                let error_count = error_count.clone();
                let input_root = input_root.clone();
                let output_root = output_root.clone();

                pool.execute(move || {
                    resize_entry(
                        flags,
                        &console_lock,
                        &error_count,
                        &input_root,
                        output_root.as_deref(),
                        image_path.as_path(),
                    );
                });
            }

            pool.join();
        }
    } else {
        resizing(flags, &console_lock, input_path.as_path(), output_path.as_deref())?;
    }

    let error_count = error_count.load(Ordering::Relaxed);

    if error_count > 0 {
        // Workers only report to stderr, so the exit code has to be set here to match the single-thread mode.
        return Err(anyhow!("{error_count} path(s) failed."));
    }

    Ok(())
}

/// Resizes one image of a directory tree. A failure is reported and counted instead of being returned, because bailing out would leave the remaining images unhandled and the thread pool holding jobs which nothing waits for.
fn resize_entry(
    flags: Flags,
    console_lock: &Mutex<()>,
    error_count: &AtomicUsize,
    input_root: &Path,
    output_root: Option<&Path>,
    image_path: &Path,
) {
    let result = map_output_path(input_root, output_root, image_path)
        .and_then(|output_path| resizing(flags, console_lock, image_path, output_path.as_deref()));

    if let Err(error) = result {
        report_error(console_lock, &error);
        error_count.fetch_add(1, Ordering::Relaxed);
    }
}

/// Checks whether a file extension belongs to an image format this program can resize.
fn is_allowed_extension(extension: &str, allow_gif: bool) -> bool {
    let allow_extensions: &[&str] =
        if allow_gif { &ALLOW_EXTENSIONS_WITH_GIF } else { &ALLOW_EXTENSIONS };

    allow_extensions.iter().any(|allow_extension| extension.eq_ignore_ascii_case(allow_extension))
}

/// Checks whether an ImageMagick format name belongs to an image format this program can resize.
fn is_allowed_format(format: &str, allow_gif: bool) -> bool {
    match format {
        "JPEG" | "PNG" | "TIFF" | "WEBP" | "PGM" => true,
        "GIF" => allow_gif,
        _ => false,
    }
}

/// Maps an image path to its destination under the output root, keeping the directory structure below the input root.
fn map_output_path(
    input_root: &Path,
    output_root: Option<&Path>,
    image_path: &Path,
) -> anyhow::Result<Option<PathBuf>> {
    match output_root {
        Some(output_root) => {
            let relative_path =
                image_path.strip_prefix(input_root).with_context(|| anyhow!("{image_path:?}"))?;

            Ok(Some(output_root.join(relative_path)))
        },
        None => Ok(None),
    }
}

/// Reads an answer to the overwrite prompt. `None` means the answer was not understood and the prompt has to be repeated.
fn parse_overwrite_answer(answer: &str) -> Option<bool> {
    match answer.trim().to_ascii_uppercase().as_str() {
        "Y" => Some(true),
        "N" => Some(false),
        _ => None,
    }
}

/// Asks whether an existing file may be replaced. The console lock is held for the whole prompt so that other threads cannot interleave their output.
fn confirm_overwrite(console_lock: &Mutex<()>, output_path: &Path) -> anyhow::Result<bool> {
    let _console_lock = console_lock.lock().unwrap();
    let mut stdout = io::stdout().lock();
    let stdin = io::stdin();
    let mut answer = String::new();

    loop {
        let _ = write!(stdout, "{output_path:?} exists, do you want to overwrite it? [Y/N] ");
        let _ = stdout.flush();

        answer.clear();

        // Nothing left to read, such as a closed stdin, leaves the file alone.
        if stdin.read_line(&mut answer).with_context(|| anyhow!("stdin"))? == 0 {
            return Ok(false);
        }

        if let Some(overwrite) = parse_overwrite_answer(answer.as_str()) {
            return Ok(overwrite);
        }
    }
}

/// Writes data through a sibling temporary file and renames it into place, so an interrupted write cannot destroy the original image.
fn write_atomically(output_path: &Path, data: &[u8]) -> anyhow::Result<()> {
    let mut temp_path = output_path.as_os_str().to_os_string();

    temp_path.push(format!(".{}.tmp", process::id()));

    let temp_path = PathBuf::from(temp_path);

    let permissions = match fs::symlink_metadata(output_path) {
        // The rename below would replace the link itself rather than write through it. Callers are expected to have settled this already, so this is the last-line guard.
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(anyhow!("{output_path:?} is a symbolic link."));
        },
        Ok(metadata) => Some(metadata.permissions()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error).with_context(|| anyhow!("{output_path:?}")),
    };

    let result = fs::write(temp_path.as_path(), data)
        .and_then(|_| match permissions {
            Some(permissions) => fs::set_permissions(temp_path.as_path(), permissions),
            None => Ok(()),
        })
        .and_then(|_| fs::rename(temp_path.as_path(), output_path));

    if result.is_err() {
        // A half-written temporary file is useless and would only litter the output directory.
        let _ = fs::remove_file(temp_path.as_path());
    }

    result.with_context(|| anyhow!("{temp_path:?}"))
}

/// Encodes an image again in its own format, resized to fit into a square of `side_maximum` pixels. The aspect ratio is preserved by `image-convert`, which computes the output size per frame.
fn encode_resized(
    output: &mut image_convert::ImageResource,
    input: &image_convert::ImageResource,
    format: &str,
    flags: Flags,
) -> Result<(), image_convert::MagickError> {
    let strip_metadata = !flags.remain_metadata;
    // A negative value lets `image-convert` pick the amount from the resize ratio, and `0` turns sharpening off.
    let sharpen = if flags.sharpen { -1f64 } else { 0f64 };
    // The two axes are set to the same value because this program does not stretch an image.
    let ppi = flags.ppi.map(|ppi| (ppi, ppi));
    // The orientation lives in the metadata only, so an image which is not turned into the orientation it asks for would come out lying on its side once the metadata is gone. It is applied even when the metadata is kept, so that the output size always matches the size the image is displayed at.
    let respect_orientation = true;

    match format {
        "JPEG" => {
            let mut config = image_convert::JPGConfig::new();

            config.strip_metadata = strip_metadata;
            config.respect_orientation = respect_orientation;
            config.width = flags.side_maximum;
            config.height = flags.side_maximum;
            config.shrink_only = flags.only_shrink;
            config.sharpen = sharpen;
            config.force_to_chroma_quartered = flags.chroma_quartered;
            // `None` asks ImageMagick for the quality it estimated from the quantization tables of the input image, so a JPEG image is not compressed a second time at a different quality.
            config.quality = flags.quality;
            config.ppi = ppi;

            image_convert::to_jpg(output, input, &config)
        },
        "PNG" => {
            let mut config = image_convert::PNGConfig::new();

            config.strip_metadata = strip_metadata;
            config.respect_orientation = respect_orientation;
            config.width = flags.side_maximum;
            config.height = flags.side_maximum;
            config.shrink_only = flags.only_shrink;
            config.sharpen = sharpen;
            config.ppi = ppi;

            image_convert::to_png(output, input, &config)
        },
        "TIFF" => {
            let mut config = image_convert::TIFFConfig::new();

            config.strip_metadata = strip_metadata;
            config.respect_orientation = respect_orientation;
            config.width = flags.side_maximum;
            config.height = flags.side_maximum;
            config.shrink_only = flags.only_shrink;
            config.sharpen = sharpen;
            config.ppi = ppi;

            image_convert::to_tiff(output, input, &config)
        },
        "WEBP" => {
            let mut config = image_convert::WEBPConfig::new();

            config.strip_metadata = strip_metadata;
            config.respect_orientation = respect_orientation;
            config.width = flags.side_maximum;
            config.height = flags.side_maximum;
            config.shrink_only = flags.only_shrink;
            config.sharpen = sharpen;
            // WEBP has no quality to read back from the input image, so the default of `image-convert` stands in when none is asked for.
            config.quality = flags.quality.unwrap_or(config.quality);
            config.ppi = ppi;

            image_convert::to_webp(output, input, &config)
        },
        "PGM" => {
            let mut config = image_convert::PGMConfig::new();

            config.strip_metadata = strip_metadata;
            config.respect_orientation = respect_orientation;
            config.width = flags.side_maximum;
            config.height = flags.side_maximum;
            config.shrink_only = flags.only_shrink;
            config.sharpen = sharpen;

            image_convert::to_pgm(output, input, &config)
        },
        "GIF" => {
            let mut config = image_convert::GIFConfig::new();

            config.strip_metadata = strip_metadata;
            config.respect_orientation = respect_orientation;
            config.width = flags.side_maximum;
            config.height = flags.side_maximum;
            config.shrink_only = flags.only_shrink;
            config.sharpen = sharpen;

            image_convert::to_gif(output, input, &config)
        },
        // The caller checks the format beforehand, so this is the last-line guard.
        _ => Err(image_convert::MagickError(format!("{format} cannot be resized."))),
    }
}

fn resizing(
    flags: Flags,
    console_lock: &Mutex<()>,
    input_path: &Path,
    output_path: Option<&Path>,
) -> anyhow::Result<()> {
    // Handing ImageMagick a path means running it through `to_string_lossy` first, which turns a name that is not UTF-8 into one that does not exist. Reading the bytes here keeps the file this program was pointed at.
    let input_data = fs::read(input_path).with_context(|| anyhow!("{input_path:?}"))?;

    let input_image_resource = image_convert::ImageResource::Data(input_data);

    let input_identify = image_convert::identify_ping(&input_image_resource)
        .with_context(|| anyhow!("{input_path:?}"))?;

    if !is_allowed_format(input_identify.format.as_str(), flags.allow_gif) {
        report_message(console_lock, format_args!("{input_path:?} is not a resizable format."));

        return Ok(());
    }

    if input_identify.has_unreadable_frames {
        // ImageMagick reads the first frame of such an image only, so writing it back would throw the animation away for good.
        report_message(
            console_lock,
            format_args!("{input_path:?} holds an animation which ImageMagick cannot read."),
        );

        return Ok(());
    }

    // The destination is settled before decoding, so that declining an overwrite does not waste a full decode.
    let output_path = match output_path {
        Some(output_path) => {
            match fs::symlink_metadata(output_path) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(anyhow!("{output_path:?} is a symbolic link."));
                },
                Ok(_) => {
                    if !flags.force && !confirm_overwrite(console_lock, output_path)? {
                        return Ok(());
                    }
                },
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    if let Some(dir_path) =
                        output_path.parent().filter(|dir_path| !dir_path.as_os_str().is_empty())
                    {
                        fs::create_dir_all(dir_path).with_context(|| anyhow!("{dir_path:?}"))?;
                    }
                },
                Err(error) => return Err(error).with_context(|| anyhow!("{output_path:?}")),
            }

            output_path
        },
        None => input_path,
    };

    let mut output_image_resource = image_convert::ImageResource::Data(Vec::new());

    encode_resized(
        &mut output_image_resource,
        &input_image_resource,
        input_identify.format.as_str(),
        flags,
    )
    .with_context(|| anyhow!("{input_path:?}"))?;

    // Only the encoded data is left to write, so the file bytes are not needed any more.
    drop(input_image_resource);

    let output_data =
        output_image_resource.into_vec().expect("the output resource is created as data");

    write_atomically(output_path, &output_data)?;

    match output_path.canonicalize() {
        // The file is already written at this point, so a failure here must not be fatal.
        Ok(canonicalized_path) => {
            report_message(console_lock, format_args!("{canonicalized_path:?} has been resized."))
        },
        Err(_) => report_message(console_lock, format_args!("{output_path:?} has been resized.")),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::*;

    // A 40x20 PNG gradient, used to check that an image really goes through the whole pipeline.
    const GRADIENT_PNG: [u8; 114] = [
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x28, 0x00, 0x00, 0x00, 0x14, 0x08, 0x02, 0x00, 0x00, 0x00, 0x70,
        0x24, 0xE8, 0xEC, 0x00, 0x00, 0x00, 0x39, 0x49, 0x44, 0x41, 0x54, 0x48, 0xC7, 0x63, 0x60,
        0x18, 0x20, 0xC0, 0xC8, 0xCB, 0xCB, 0x3B, 0x20, 0x16, 0xB3, 0xF0, 0xF1, 0xF1, 0x0D, 0x8C,
        0xC5, 0xA3, 0x3E, 0x1E, 0xF5, 0xF1, 0x68, 0x50, 0x8F, 0xFA, 0x78, 0x34, 0x71, 0x8D, 0x06,
        0xF5, 0xA8, 0x8F, 0xE1, 0x0D, 0x81, 0x4F, 0x9F, 0x3E, 0x0D, 0x8C, 0xC5, 0xFF, 0xFF, 0xFF,
        0x1F, 0x10, 0x8B, 0x01, 0x3F, 0x29, 0x08, 0xC3, 0x35, 0xAE, 0x74, 0xF7, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    /// A directory under the system temporary directory which removes itself when it goes out of scope.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> TempDir {
            static COUNTER: AtomicUsize = AtomicUsize::new(0);

            let path = env::temp_dir().join(format!(
                "image-resizer-{}-{}",
                process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));

            fs::create_dir_all(path.as_path()).unwrap();

            TempDir(path)
        }

        fn join(&self, file_name: &str) -> PathBuf {
            self.0.join(file_name)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(self.0.as_path());
        }
    }

    #[test]
    fn allowed_extensions() {
        assert!(is_allowed_extension("jpg", false));
        assert!(is_allowed_extension("JPG", false));
        assert!(is_allowed_extension("jpeg", false));
        assert!(is_allowed_extension("Jpeg", false));
        assert!(is_allowed_extension("png", false));
        assert!(is_allowed_extension("PNG", false));
        assert!(is_allowed_extension("webp", false));
        assert!(is_allowed_extension("tif", false));
        assert!(is_allowed_extension("tiff", false));
        assert!(is_allowed_extension("pgm", false));

        assert!(!is_allowed_extension("bmp", false));
        assert!(!is_allowed_extension("ico", false));
    }

    #[test]
    fn allowed_gif_extension() {
        assert!(!is_allowed_extension("gif", false));

        assert!(is_allowed_extension("gif", true));
        assert!(is_allowed_extension("GIF", true));
    }

    #[test]
    fn allowed_formats() {
        assert!(is_allowed_format("JPEG", false));
        assert!(is_allowed_format("PNG", false));
        assert!(is_allowed_format("TIFF", false));
        assert!(is_allowed_format("WEBP", false));
        assert!(is_allowed_format("PGM", false));

        assert!(!is_allowed_format("BMP", false));
        assert!(!is_allowed_format("ICO", false));
    }

    #[test]
    fn allowed_gif_format() {
        assert!(!is_allowed_format("GIF", false));

        assert!(is_allowed_format("GIF", true));
    }

    #[test]
    fn output_path_keeps_directory_structure() {
        let output_path = map_output_path(
            Path::new("/input"),
            Some(Path::new("/output")),
            Path::new("/input/a/b/image.png"),
        )
        .unwrap();

        assert_eq!(Some(PathBuf::from("/output/a/b/image.png")), output_path);
    }

    #[test]
    fn output_path_is_absent_without_an_output_root() {
        let output_path =
            map_output_path(Path::new("/input"), None, Path::new("/input/image.png")).unwrap();

        assert_eq!(None, output_path);
    }

    #[test]
    fn overwrite_answers() {
        assert_eq!(Some(true), parse_overwrite_answer("y"));
        assert_eq!(Some(true), parse_overwrite_answer("Y"));
        assert_eq!(Some(true), parse_overwrite_answer("y "));

        assert_eq!(Some(false), parse_overwrite_answer("n"));
        assert_eq!(Some(false), parse_overwrite_answer("N"));
        assert_eq!(Some(false), parse_overwrite_answer(" N"));

        assert_eq!(None, parse_overwrite_answer(""));
        assert_eq!(None, parse_overwrite_answer("maybe"));
    }

    #[test]
    fn write_atomically_creates_a_new_file() {
        let temp_dir = TempDir::new();
        let output_path = temp_dir.join("image.png");

        write_atomically(output_path.as_path(), b"resized").unwrap();

        assert_eq!(b"resized".to_vec(), fs::read(output_path.as_path()).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn write_atomically_keeps_the_mode_of_the_replaced_file() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = TempDir::new();
        let output_path = temp_dir.join("image.png");

        fs::write(output_path.as_path(), b"original").unwrap();
        fs::set_permissions(output_path.as_path(), fs::Permissions::from_mode(0o600)).unwrap();

        write_atomically(output_path.as_path(), b"resized").unwrap();

        assert_eq!(b"resized".to_vec(), fs::read(output_path.as_path()).unwrap());
        assert_eq!(
            0o600,
            fs::metadata(output_path.as_path()).unwrap().permissions().mode() & 0o777
        );
    }

    #[test]
    fn resizing_fits_an_image_into_the_side_maximum() {
        let temp_dir = TempDir::new();
        let input_path = temp_dir.join("gradient.png");

        fs::write(input_path.as_path(), GRADIENT_PNG).unwrap();

        let console_lock = Mutex::new(());

        let flags = Flags {
            allow_gif:        false,
            remain_metadata:  false,
            force:            true,
            side_maximum:     10,
            only_shrink:      false,
            sharpen:          true,
            quality:          None,
            ppi:              None,
            chroma_quartered: false,
        };

        resizing(flags, &console_lock, input_path.as_path(), None).unwrap();

        let identify = image_convert::identify_ping(&image_convert::ImageResource::Data(
            fs::read(input_path.as_path()).unwrap(),
        ))
        .unwrap();

        // The longer side is scaled down to the maximum, and the aspect ratio decides the other one.
        assert_eq!(10, identify.resolution.width);
        assert_eq!(5, identify.resolution.height);
    }
}
