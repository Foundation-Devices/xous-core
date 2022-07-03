mod generate;

use anyhow::Context;
use clap::{App, Arg};
use std::fs::File;
use std::io::{Read, Write};

fn main() -> anyhow::Result<()> {
    let matches = App::new("svd2repl")
        .about("Generate a Renode Platform description from SVD files")
        .arg(
            Arg::with_name("input")
                .help("Input SVD file")
                .short("i")
                .takes_value(true)
                .value_name("FILE"),
        )
        .arg(
            Arg::with_name("output")
                .help("Output .repl file or directory")
                .short("o")
                .takes_value(true)
                .value_name("FILE"),
        )
        .version(concat!(
            env!("CARGO_PKG_VERSION"),
            include_str!(concat!(env!("OUT_DIR"), "/commit-info.txt"))
        ))
        .get_matches();

    let src: Box<dyn Read> = match matches.value_of("input") {
        Some(file) => Box::new(File::open(file).context("Cannot open the SVD file")?),
        None => Box::new(std::io::stdin()),
    };

    let mut dest: Box<dyn Write> = match matches.value_of("output") {
        None => Box::new(std::io::stdout()),
        Some(path) => Box::new(File::create(path).context("Cannot open destination file")?),
    };

    generate::generate(src, &mut dest).context("Cannot generate output file")?;

    Ok(())
}
