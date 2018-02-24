// A simple app to read values from the rainforest cloud and export them as
// prometheus metrics

extern crate clap;
use clap::{App, Arg};

#[macro_use]
extern crate serde_derive;
extern crate serde_yaml;

extern crate hyper;
extern crate prometheus;

use hyper::header::ContentType;
use hyper::mime::Mime;
use hyper::server::{Request, Response, Server};

use std::net;

mod config {
    use std::fs::File;

    use serde_yaml;

    #[derive(Debug, Deserialize)]
    pub struct Config {
        pub server: Server,
    }

    #[derive(Debug, Deserialize)]
    pub struct Server {
        pub port: u16,
    }

    impl Config {
        pub fn new(filename: &str) -> Config {
            serde_yaml::from_reader(File::open(filename).unwrap()).unwrap()
        }
    }
}

fn main() {
    let matches = App::new("Prom Rain")
        .version("0.1.0")
        .author("Hyrum Wright <hyrum@hyrumwright.org>")
        .arg(
            Arg::with_name("config")
                .long("config")
                .help("File to use for configuration")
                .default_value("config.yml")
                .takes_value(true),
        )
        .get_matches();

    let config = matches.value_of("config").unwrap();
    let config = config::Config::new(config);
    println!("Config: {:?}", config);

    let addr = "0.0.0.0".parse().unwrap();
    let addr = net::SocketAddr::new(addr, config.server.port);
    Server::http(addr)
        .unwrap()
        .handle(move |req: Request, res: Response| {})
        .unwrap();
}
