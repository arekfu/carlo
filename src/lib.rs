extern crate irc;
#[macro_use] extern crate log;
extern crate reqwest;
#[macro_use] extern crate serde_derive;
extern crate toml;

pub use carlo::Carlo;
mod carlo;
mod config;
mod irc_listener;
mod j_listener;
