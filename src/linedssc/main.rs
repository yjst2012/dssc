extern crate dssc;
extern crate byteorder;
extern crate clap;

use dssc::{DSSCDecoder, DSSCEncoder, Compressor};
use dssc::chunked::ChunkedCompressor;
use dssc::convolve::ConvolveCompressor;
use dssc::flate::FlateCompressor;
use dssc::varint::{put_uvarint, read_uvarint};
use std::env;
use std::ops::DerefMut;
use std::io::{stdin, stdout, Read, Write, Error};
use clap::{Arg, App};

const DEFAULT_THRESHOLD: f32 = 0.5;

fn encode(threshold: f32, comp: &mut Compressor) -> Result<(), Error> {
    let mut encoder = DSSCEncoder::new(comp, threshold);
    let mut len_buf = [0; 10];
    loop {
        let mut input = String::new();
        let n = stdin().read_line(&mut input)?;
        if n == 0 {
            return Ok(());
        }
        let encoded = encoder.encode(input.as_bytes());
        let len_len = put_uvarint(&mut len_buf, encoded.len() as u64);
        stdout().write(&len_buf[0..len_len])?;
        stdout().write(&encoded)?;
    }
}

fn decode(threshold: f32, comp: &mut Compressor) -> Result<(), Error> {
    let mut decoder = DSSCDecoder::new(comp, threshold);
    loop {
        let mut buf = Vec::new();
        let len = read_uvarint(&mut stdin())?;
        let n = stdin().take(len as u64).read_to_end(&mut buf)?;
        if n == 0 {
            return Ok(());
        }
        stdout().write_all(&decoder.decode(&buf))?;
    }
}

fn main() {
    let matches = App::new("Linefed Discreete Stream Compressor")
        .version("0.0")
        .author("Denis Lavrov <bahus.vel@gmail.com>")
        .about("Compresses stream of lines")
        .arg(
            Arg::with_name("threshold")
                .short("t")
                .long("threshold")
                .help("Sets insert threshold for history cache")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("decompress")
                .short("d")
                .long("decompress")
                .help("Switches linedssc into decompress mode"),
        )
        .arg(
            Arg::with_name("algorithm")
                .short("a")
                .long("algorithm")
                .possible_values(&["convolve", "chunked", "flate"])
                .default_value("chunked")
                .help("Switches linedssc to use a different algorithm")
                .takes_value(true),
        )
        .get_matches();

    let mut comp: Box<Compressor> = match matches.value_of("algorithm") {
        Some("convolve") => Box::new(ConvolveCompressor {}),
        Some("chunked") => Box::new(ChunkedCompressor {}),
        Some("flate") => Box::new(FlateCompressor::new()),
        Some(_) | None => panic!("Cannot be none"),
    };

    let threshold = matches
        .value_of("threshold")
        .map(|t| t.parse().expect("Incorrect format for threshold"))
        .unwrap_or(DEFAULT_THRESHOLD);

    if matches.is_present("decompress") {
        if let Err(error) = decode(threshold, comp.deref_mut()) {
            eprintln!("error: {}", error);
        }
        return;
    }
    if let Err(error) = encode(threshold, comp.deref_mut()) {
        eprintln!("error: {}", error);
    }
}
