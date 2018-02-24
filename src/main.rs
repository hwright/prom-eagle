// A simple app to read values from the rainforest cloud and export them as
// prometheus metrics

extern crate clap;
extern crate hyper;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate prometheus;
#[macro_use]
extern crate serde_derive;
extern crate serde_yaml;
#[macro_use]
extern crate log;
extern crate env_logger;

use clap::{App, Arg};
use hyper::header::ContentType;
use hyper::mime::Mime;
use hyper::server::{Request, Response, Server};
use prometheus::{Counter, Encoder, TextEncoder};

use std::net;

mod config {
    use std::fs::File;

    use serde_yaml;

    #[derive(Debug, Deserialize)]
    pub struct Config {
        pub server: Server,
        pub eagle: Eagle,
    }

    #[derive(Debug, Deserialize)]
    pub struct Server {
        pub port: u16,
    }

    #[derive(Debug, Deserialize)]
    pub struct Eagle {
        pub user: String,
        pub password: String,
    }

    impl Config {
        pub fn new(filename: &str) -> Config {
            serde_yaml::from_reader(File::open(filename).unwrap()).unwrap()
        }
    }
}

lazy_static! {
    static ref HTTP_COUNTER: Counter = register_counter!(
        opts!(
            "example_http_requests_total",
            "Total number of HTTP requests made.",
            labels!{"handler" => "all",}
        )
    ).unwrap();
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

    let encoder = TextEncoder::new();
    let addr = "0.0.0.0".parse().unwrap();
    let addr = net::SocketAddr::new(addr, config.server.port);
    println!("Starting server for {}", addr);
    Server::http(addr)
        .unwrap()
        .handle(move |req: Request, mut res: Response| {
            let metric_families = prometheus::gather();
            let mut buffer = vec![];
            encoder.encode(&metric_families, &mut buffer).unwrap();
            res.headers_mut()
                .set(ContentType(encoder.format_type().parse::<Mime>().unwrap()));
            res.send(&buffer).unwrap();
        })
        .unwrap();
}
