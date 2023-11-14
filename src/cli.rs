use std::path::PathBuf;

use clap::{CommandFactory, FromArgMatches, Parser};
use concat_with::concat_line;
use terminal_size::terminal_size;

const APP_NAME: &str = "Image Resizer";
const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
const CARGO_PKG_AUTHORS: &str = env!("CARGO_PKG_AUTHORS");

const AFTER_HELP: &str = "Enjoy it! https://magiclen.org";

const APP_ABOUT: &str = concat!(
    "It helps you interlace an image or multiple images for web-page usage.\n\nEXAMPLES:\n",
    concat_line!(prefix "image-resizer ",
        "/path/to/image -m 1920                           # Make /path/to/image resized",
        "/path/to/folder -m 1920                          # Make images inside /path/to/folder and make resized",
        "/path/to/image -o /path/to/image2 -m 1920        # Make /path/to/image resized, and save it to /path/to/image2",
        "/path/to/folder -o /path/to/folder2 -m 1920      # Make images inside /path/to/folder resized, and save them to /path/to/folder2",
        "/path/to/folder -o /path/to/folder2 -f -m 1920   # Make images inside /path/to/folder resized, and save them to /path/to/folder2 without overwriting checks",
        "/path/to/folder --allow-gif -r -m 1920           # Make images inside /path/to/folder including GIF resized and also remain their profiles",
        "/path/to/image -m 1920 --shrink                  # Make /path/to/image shrunk if it needs to be",
        "/path/to/image -m 1920 -q 75                     # Make /path/to/image resized with a quality of 75 if it uses lossy compression",
        "/path/to/image -m 1920 --4:2:0                   # Make /path/to/image resized and output using 4:2:0 (chroma quartered) subsampling to reduce the file size",
        "/path/to/image -m 1920 --no-sharpen              # Make /path/to/image resized without auto sharpening",
        "/path/to/image -m 1920 --ppi 150                 # Make /path/to/image resized, and set their PPI to 150",
    )
);

#[derive(Debug, Parser)]
#[command(name = APP_NAME)]
#[command(term_width = terminal_size().map(|(width, _)| width.0 as usize).unwrap_or(0))]
#[command(version = CARGO_PKG_VERSION)]
#[command(author = CARGO_PKG_AUTHORS)]
#[command(after_help = AFTER_HELP)]
pub struct CLIArgs {
    #[arg(value_hint = clap::ValueHint::AnyPath)]
    #[arg(help = "Assign an image or a directory for image resizing. It should be a path of a \
                  file or a directory")]
    pub input_path:       PathBuf,
    #[arg(short, long, visible_alias = "output")]
    #[arg(value_hint = clap::ValueHint::AnyPath)]
    #[arg(help = "Assign a destination of your generated files. It should be a path of a \
                  directory or a file depending on your input path")]
    pub output_path:      Option<PathBuf>,
    #[arg(short, long)]
    #[arg(help = "Use only one thread")]
    pub single_thread:    bool,
    #[arg(short, long)]
    #[arg(help = "Force to overwrite files")]
    pub force:            bool,
    #[arg(long)]
    #[arg(help = "Allow to do GIF interlacing")]
    pub allow_gif:        bool,
    #[arg(short, long)]
    #[arg(help = "Remain the profiles of all images")]
    pub remain_profile:   bool,
    #[arg(short = 'm', long, visible_alias = "max")]
    #[arg(
        help = "Set the maximum pixels of each side of an image (Aspect ratio will be preserved)"
    )]
    pub side_maximum:     u16,
    #[arg(long, visible_alias = "shrink")]
    #[arg(help = "Only shrink images, not enlarge them")]
    pub only_shrink:      bool,
    #[arg(long)]
    #[arg(help = "Disable automatically sharpening")]
    pub no_sharpen:       bool,
    #[arg(short, long)]
    #[arg(default_value = "92")]
    #[arg(value_parser = clap::value_parser!(u8).range(0..=100))]
    #[arg(help = "Set the quality for lossy compression")]
    pub quality:          u8,
    #[arg(long)]
    #[arg(value_parser = parse_ppi)]
    #[arg(help = "Set pixels per inch (ppi)")]
    pub ppi:              Option<f64>,
    #[arg(long, visible_alias = "4:2:0")]
    #[arg(help = "Use 4:2:0 (chroma quartered) subsampling to reduce the file size if it is \
                  supported")]
    pub chroma_quartered: bool,
}

fn parse_ppi(arg: &str) -> Result<f64, String> {
    let ppi = arg.parse::<f64>().map_err(|err| err.to_string())?;

    if ppi <= 0f64 {
        return Err("PPI must be bigger than 0".into());
    }

    Ok(ppi)
}

pub fn get_args() -> CLIArgs {
    let args = CLIArgs::command();

    let about = format!("{APP_NAME} {CARGO_PKG_VERSION}\n{CARGO_PKG_AUTHORS}\n{APP_ABOUT}");

    let args = args.about(about);

    let matches = args.get_matches();

    match CLIArgs::from_arg_matches(&matches) {
        Ok(args) => args,
        Err(err) => {
            err.exit();
        },
    }
}
