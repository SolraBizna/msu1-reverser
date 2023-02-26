use std::{
    fs::File,
    io::{Read, Write, BufWriter},
    path::PathBuf,
};

use rand::prelude::*;

// Natural logarithm of the quietest volume we consider audible.
const SILENT_LOG: f32 = -6.0;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = Some("Given an MSU-1 formatted .pcm file, creates a reversed version of that file. Information is lost; if there's an original introduction, it isn't transferred to the new file, and the loop is offsetted by a few seconds!") )]
struct Invocation {
    /// Input PCM file
    #[arg()]
    infile: PathBuf,
    /// Output PCM file
    #[arg()]
    outfile: PathBuf,
    /// Number of seconds of fade in (doesn't apply to tracks with a zero loop
    /// point)
    #[arg(short, long, default_value = "3.0")]
    fade_time: f32,
}

fn read_header<T: Read>(file: &mut T) -> Option<u32> {
    let mut buf = [0u8; 8];
    file.read_exact(&mut buf).expect("Unable to read input file header");
    if &buf[0..4] != b"MSU1" {
        panic!("Input file is not an MSU-1 PCM file");
    }
    match u32::from_le_bytes(buf[4..8].try_into().unwrap()) {
        0 => None,
        x => Some(x)
    }
}

fn write_reversed<T: Write>(outfile: &mut T, buf: &[u8]) {
    for chunk in buf.chunks(4).rev() {
        outfile.write_all(chunk).expect("Unable to write audio data");
    }
}

fn write_reversed_with_fadein<T: Write>(outfile: &mut T, buf: &[u8], fade_samples: usize) {
    let mut rng = thread_rng();
    // one half bit of dither
    let distribution = rand::distributions::Uniform::new_inclusive(-32768, 32768);
    let mut iter = buf.chunks(4).rev().cycle();
    let mut fade_rem = fade_samples;
    for chunk in &mut iter {
        let left = i16::from_le_bytes(chunk[0..2].try_into().unwrap());
        let right = i16::from_le_bytes(chunk[0..2].try_into().unwrap());
        let fade_magnitude = ((SILENT_LOG * (fade_rem as f32) / (fade_samples as f32)).exp() * 65536.0) as i32;
        let left = ((left as i32 * fade_magnitude + rng.sample(distribution)) >> 16) as i16;
        let right = ((right as i32 * fade_magnitude + rng.sample(distribution)) >> 16) as i16;
        let mut faded_chunk = [0u8; 4];
        faded_chunk[0..2].clone_from_slice(&left.to_le_bytes());
        faded_chunk[2..4].clone_from_slice(&right.to_le_bytes());
        outfile.write_all(&faded_chunk).expect("Unable to write audio data");
        fade_rem -= 1;
        if fade_rem == 0 {
            break;
        }
    }
    let mut rem_chunks = buf.len() / 4;
    for chunk in iter {
        outfile.write_all(chunk).expect("Unable to write audio data");
        rem_chunks -= 1;
        if rem_chunks == 0 {
            break;
        }
    }
}



fn main() {
    let invocation = Invocation::parse();
    if !invocation.fade_time.is_finite() || invocation.fade_time < 0.0 {
        panic!("Invalid fade time. Must be positive.");
    }
    else if invocation.fade_time > 600.0 {
        panic!("Ridiculously long fade time.");
    }
    let fade_samples = (invocation.fade_time * 44100.0 + 0.5).floor() as usize;
    let mut infile = File::open(&invocation.infile).expect("Unable to open input file");
    let loop_point = read_header(&mut infile);
    let mut all = vec![];
    infile.read_to_end(&mut all).expect("Unable to read input file");
    if all.len() % 4 != 0 {
        panic!("Input file has been corrupted, or has had extra data added!");
    }
    let mut outfile = BufWriter::new(File::create(&invocation.outfile).expect("Unable to open output file"));
    outfile.write_all(b"MSU1").expect("Unable to write output header");
    match loop_point {
        None => {
            // It's simple, we reverse the Batman
            outfile.write_all(&[0u8;4]).expect("Unable to write output header");
            write_reversed(&mut outfile, &all);
        },
        Some(loop_point) => {
            outfile.write_all(&fade_samples.to_le_bytes()).expect("Unable to write output header");
            let start_offset = (loop_point as usize) * 4;
            let all = &all[start_offset..];
            write_reversed_with_fadein(&mut outfile, all, fade_samples);
        },
    }
}
