use classy::decompile_jar;
use glob::glob;
use regex::Regex;
use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::process::exit;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "classy", about = "Java class file utilities")]
enum Opt {
    DecompileJar {
        /// Activate debug mode
        // short and long flags (-d, --debug) will be deduced from the field's name
        #[structopt(short, long)]
        verbose: bool,

        /// Input JAR file
        #[structopt(parse(from_os_str))]
        input: PathBuf,

        /// Output directory
        #[structopt(parse(from_os_str))]
        output: PathBuf,
    },
    SearchJars {
        /// Directory containing jar files to search
        #[structopt(parse(from_os_str))]
        input: PathBuf,

        /// Regular expression
        #[structopt(short, long)]
        pattern: String,
    },
}

fn main() -> io::Result<()> {
    match Opt::from_args() {
        Opt::DecompileJar {
            verbose,
            input,
            output,
        } => {
            let f = File::open(input)?;
            match decompile_jar(f, &output, verbose) {
                Ok(_) => {}
                Err(e) => {
                    println!("{}", e);
                    exit(-1);
                }
            }
        }
        Opt::SearchJars { input, pattern } => {
            let re = Regex::new(&pattern).unwrap();
            let glob_pattern = format!("{}/*.jar", input.display());
            for entry in glob(&glob_pattern).expect("Failed to read glob pattern") {
                match entry {
                    Ok(path) => {
                        let mut found = false;
                        let name = format!("{:?}", path.display());
                        let f = File::open(path)?;
                        let mut zip = zip::ZipArchive::new(f)?;
                        for i in 0..zip.len() {
                            let file = zip.by_index(i)?;
                            if file.name().ends_with(".class") {
                                //let class = read_class(&mut file)?;
                                if re.is_match(file.name()) {
                                    if !found {
                                        println!("Found matches in {name}:");
                                        found = true;
                                    }
                                    println!("MATCH: {}", file.name());
                                }
                            }
                        }
                    }
                    Err(e) => println!("{:?}", e),
                }
            }
        }
    }
    Ok(())
}
