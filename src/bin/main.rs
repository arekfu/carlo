extern crate carlo;
extern crate simplelog;

use simplelog::{CombinedLogger, Config, LevelFilter, TermLogger, WriteLogger};
use std::fs::File;

fn main() {
    CombinedLogger::init(vec![
        TermLogger::new(LevelFilter::Info, Config::default()).unwrap(),
        WriteLogger::new(
            LevelFilter::Info,
            Config::default(),
            File::create("carlo.log").unwrap(),
        ),
    ]).unwrap();
    let mut carlo = carlo::Carlo::new();
    carlo.run();
}
