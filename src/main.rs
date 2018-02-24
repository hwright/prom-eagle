// A simple app to read values from the rainforest cloud and export them as
// prometheus metrics

extern crate clap;
extern crate env_logger;
extern crate futures;
extern crate hyper;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
#[macro_use]
extern crate prometheus;
#[macro_use]
extern crate serde_derive;
extern crate serde_yaml;

use clap::{App, Arg};
use futures::future::Future;
use hyper::header::{ContentLength, ContentType};
use hyper::mime::Mime;
use hyper::server::{Http, Request, Response, Service};
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

struct MetricsService;

impl Service for MetricsService {
    // boilerplate hooking up hyper's server types
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;

    // The future representing the eventual Response your call will
    // resolve to. This can change to whatever Future you need.
    type Future = Box<Future<Item = Self::Response, Error = Self::Error>>;

    fn call(&self, _req: Request) -> Self::Future {
        let encoder = TextEncoder::new();
        let metric_families = prometheus::gather();
        let mut buffer = vec![];
        encoder.encode(&metric_families, &mut buffer).unwrap();
        // We're currently ignoring the Request
        // And returning an 'ok' Future, which means it's ready
        // immediately, and build a Response with the 'PHRASE' body.
        Box::new(futures::future::ok(
            Response::new()
                .with_header(ContentType(encoder.format_type().parse::<Mime>().unwrap()))
                .with_header(ContentLength(buffer.len() as u64))
                .with_body(buffer),
        ))
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

    let addr = "0.0.0.0".parse().unwrap();
    let addr = net::SocketAddr::new(addr, config.server.port);
    println!("Starting server for {}", addr);
    let server = Http::new().bind(&addr, || Ok(MetricsService)).unwrap();
    server.run().unwrap();
}
