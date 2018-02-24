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
use hyper::{Method, StatusCode};
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

struct MetricsService {
    encoder: TextEncoder,
}

impl MetricsService {
    pub fn new() -> MetricsService {
        MetricsService {
            encoder: TextEncoder::new(),
        }
    }
}

impl Service for MetricsService {
    // boilerplate hooking up hyper's server types
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;

    // The future representing the eventual Response your call will
    // resolve to. This can change to whatever Future you need.
    type Future = Box<Future<Item = Self::Response, Error = Self::Error>>;

    fn call(&self, req: Request) -> Self::Future {
        let mut response = Response::new();

        match (req.method(), req.path()) {
            (&Method::Get, "/metrics") => {
                HTTP_COUNTER.inc();
                let metric_families = prometheus::gather();
                let mut buffer = vec![];
                self.encoder.encode(&metric_families, &mut buffer).unwrap();
                response.headers_mut().set(ContentType(
                    self.encoder.format_type().parse::<Mime>().unwrap(),
                ));
                response
                    .headers_mut()
                    .set(ContentLength(buffer.len() as u64));
                response.set_body(buffer);
            }
            _ => {
                response.set_status(StatusCode::NotFound);
            }
        }
        Box::new(futures::future::ok(response))
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
    let server = Http::new()
        .bind(&addr, || Ok(MetricsService::new()))
        .unwrap();
    server.run().unwrap();
}
