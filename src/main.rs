// A simple app to read values from the rainforest cloud and export them as
// prometheus metrics

extern crate clap;
extern crate env_logger;
extern crate futures;
#[macro_use]
extern crate hyper;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
#[macro_use]
extern crate prometheus;
extern crate reqwest;
#[macro_use]
extern crate serde_derive;
extern crate serde_yaml;

use clap::{App, Arg};
use futures::future::Future;
use hyper::{Method, StatusCode};
use hyper::header::{ContentLength, ContentType};
use hyper::mime::Mime;
use hyper::server::{Http, Request, Response, Service};
use prometheus::{Encoder, Gauge, TextEncoder};

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
        pub cloud_id: String,
    }

    impl Config {
        pub fn new(filename: &str) -> Config {
            serde_yaml::from_reader(File::open(filename).unwrap()).unwrap()
        }
    }
}

mod client {
    use super::config;
    use super::INSTANT_POWER;
    use reqwest;

    header! { (User, "User") => [String] }
    header! { (Password, "Password") => [String] }
    header! { (CloudId, "Cloud-Id") => [String] }

    #[derive(Debug, Deserialize)]
    #[allow(non_snake_case)]
    struct EagleDemand {
        DeviceMacId: String,
        MeterMacId: String,
        TimeStamp: String,
        Demand: String,
        Multiplier: String,
        Divisor: String,
        DigitsRight: String,
        DigitsLeft: String,
        SuppressLeadingZero: String,
    }

    impl EagleDemand {
        /// Returns the power represented by this result (in watts)
        fn get_power(&self) -> f64 {
            let demand = i64::from_str_radix(&self.Demand[2..], 16).unwrap();
            let multiplier = i64::from_str_radix(&self.Demand[2..], 16).unwrap();
            let divisor = i64::from_str_radix(&self.Divisor[2..], 16).unwrap();
            let factor = divisor as f64 / 1000.0;
            debug!("Demand:     {}", demand);
            debug!("Multiplier: {}", multiplier);
            debug!("Divisor:    {}", divisor);
            debug!("Factor:     {}", factor);
            debug!("Result:     {}", (demand * multiplier) as f64 * factor);
            (demand * multiplier) as f64 * factor
        }
    }

    #[derive(Debug, Deserialize)]
    #[allow(non_snake_case)]
    struct EagleResponse {
        pub InstantaneousDemand: EagleDemand,
    }

    pub struct EagleClient<'a> {
        config: &'a config::Eagle,
        client: reqwest::Client,
    }

    impl<'a> EagleClient<'a> {
        pub fn new(config: &'a config::Eagle) -> EagleClient<'a> {
            EagleClient {
                config: config,
                client: reqwest::Client::new(),
            }
        }

        pub fn update_metrics(&self) {
            let mut resp = self.client
                .post("https://rainforestcloud.com:9445/cgi-bin/post_manager")
                .header(User(self.config.user.clone()).to_owned())
                .header(Password(self.config.password.clone()).to_owned())
                .header(CloudId(self.config.cloud_id.clone()).to_owned())
                .body(
                    "<Command><Name>get_instantaneous_demand</Name><Format>JSON</Format></Command>",
                )
                .send()
                .unwrap();
            let resp: EagleResponse = resp.json().unwrap();
            INSTANT_POWER.set(resp.InstantaneousDemand.get_power());
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
    type Future = Box<Future<Item = Self::Response, Error = Self::Error>>;

    fn call(&self, req: Request) -> Self::Future {
        let mut response = Response::new();

        match (req.method(), req.path()) {
            (&Method::Get, "/metrics") => {
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
    static ref INSTANT_POWER: Gauge = register_gauge!(
        opts!(
            "instantaneous_power",
            "Instantaneous electricity usage.",
            labels!{"handler" => "all",}
        )
    ).unwrap();
}

fn main() {
    env_logger::init();

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

    let eagle_client = client::EagleClient::new(&config.eagle);
    eagle_client.update_metrics();

    let addr = "0.0.0.0".parse().unwrap();
    let addr = net::SocketAddr::new(addr, config.server.port);
    info!("Starting server for {}", addr);
    let server = Http::new()
        .bind(&addr, || Ok(MetricsService::new()))
        .unwrap();
    server.run().unwrap();
}
