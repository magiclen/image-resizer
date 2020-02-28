extern crate image_resizer;

use std::process;

use image_resizer::*;

fn main() {
    let config = Config::from_cli();

    match config {
        Ok(config) => {
            match run(config) {
                Ok(es) => {
                    process::exit(es);
                }
                Err(error) => {
                    eprintln!("{}", error);
                    process::exit(-1);
                }
            }
        }
        Err(err) => {
            eprintln!("{}", err);
            process::exit(-1);
        }
    }
}
