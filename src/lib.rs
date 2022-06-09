extern crate irc;
#[macro_use]
extern crate log;
extern crate reqwest;
#[macro_use]
extern crate serde_derive;
extern crate toml;

#[cfg(test)]
#[macro_use]
extern crate proptest;

pub use crate::carlo::Carlo;
mod carlo;
mod config;
