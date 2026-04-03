use anyhow::Context;
use clap::{Parser, Subcommand};
use flate2::read::{GzDecoder, ZlibDecoder};
#[allow(unused_imports)]
use std::fs;
use std::{
    ffi::CStr,
    io::{BufReader, prelude::*},
};
/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    // /// Name of the person to greet
    // #[arg(short, long)]
    // name: String,

    // /// Number of times to greet
    // #[arg(short, long, default_value_t = 1)]
    // count: u8,
    #[command(subcommand)]
    command: Command,
}

/// Doc comment
#[derive(Debug, Subcommand)]
enum Command {
    /// Doc comment
    Init,
    CatFile {
        #[clap(short = 'p')]
        pretty_print: bool,

        object_hash: String,
    },
}

enum Kind {
    Blob,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Init => {
            fs::create_dir(".git").unwrap();
            fs::create_dir(".git/objects").unwrap();
            fs::create_dir(".git/refs").unwrap();
            fs::write(".git/HEAD", "ref: refs/heads/main\n").unwrap();
            println!("Initialized git directory")
        }
        Command::CatFile {
            pretty_print,
            object_hash,
        } => {
            // todo: support shortest uniq hashes
            let f = std::fs::File::open(format!(
                ".git/objects/{}/{}",
                &object_hash[..2],
                &object_hash[2..]
            ))
            .context("open in .git/objects")?;
            let z = ZlibDecoder::new(f);
            let mut z = BufReader::new(z);
            let mut buf = Vec::new();
            z.read_until(0, &mut buf)
                .context("read header from .git/objects")?;
            let header = CStr::from_bytes_with_nul(&buf)
                .expect("know there is exactly one nul, and it's at the end");
            let header = header
                .to_str()
                .context(".git/objectss file header isn't valid UTF-8")?;

            let Some((kind, size)) = header.split_once(' ') else {
                anyhow::bail!(".git/objects file header did not start with known type: {header}");
            };

            let kind = match kind {
                "blob" => Kind::Blob,
                _ => {
                    anyhow::bail!(".git/objects file header has unknown type: {kind}");
                }
            };

            let size = size
                .parse::<usize>()
                .context(".git/objects file header has invalid size: {size}")?;
            buf.clear();
            // buf.reserve_exact(size);
            buf.resize(size, 0);
            z.read_exact(&mut buf[..])
                .context("read true content from .git/objects file is too short")?;
            let n = z.read(&mut [0]).context("read trailing nul")?;
            anyhow::ensure!(n == 0, "trailing nul in .git/objects file, had {n}");

            let stdout = std::io::stdout();
            let mut stdout = stdout.lock();

            match kind {
                Kind::Blob => {
                    stdout
                        .write_all(&buf)
                        .context("write objects content to stdout")?;
                }
            }
        }
    }

    Ok(())
    // if args[1] == "cat-file" && args[2] == "-p" && args.len() == 4 {
    //     let mut d = GzDecoder::new(args[3].as_bytes());
    //     let mut s = String::new();
    //     d.read_to_string(&mut s).unwrap();
    //     println!("{}", s);
    // } else {
    //     println!("unknown command: {}", args[1])
    // }
}
