// A simple app to read values from the rainforest cloud and export them as
// prometheus metrics

extern crate clap;
extern crate env_logger;
#[macro_use]
extern crate error_chain;
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

    error_chain!{
        foreign_links {
            Io(::std::io::Error);
            Json(serde_yaml::Error);
        }
    }

    #[derive(Debug, Deserialize)]
    pub struct Config {
        pub server: Server,
        pub eagle: Eagle,
    }

    #[derive(Debug, Deserialize)]
    pub struct Server {
        pub port: u16,
    }

    #[derive(Debug, Deserialize, Clone)]
    pub struct Eagle {
        pub user: String,
        pub password: String,
        pub cloud_id: String,
        pub update_interval_secs: u32,
    }

    impl Config {
        pub fn new(filename: &str) -> Result<Config> {
            Ok(serde_yaml::from_reader(File::open(filename)?)?)
        }
    }
}

mod client {
    use super::config;
    use super::INSTANT_POWER;
    use reqwest;

    use std::num::ParseIntError;
    use std::thread;
    use std::time::Duration;

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
        fn get_power(&self) -> Result<f64, ParseIntError> {
            let demand = i64::from_str_radix(&self.Demand[2..], 16)?;
            let multiplier = i64::from_str_radix(&self.Multiplier[2..], 16)?;
            let divisor = i64::from_str_radix(&self.Divisor[2..], 16)?;
            let factor = divisor as f64 / 1000.0;
            let result = (demand * multiplier) as f64 * factor;
            debug!("Demand:     {}", demand);
            debug!("Multiplier: {}", multiplier);
            debug!("Divisor:    {}", divisor);
            debug!("Factor:     {}", factor);
            debug!("Result:     {}", result);
            Ok(result)
        }
    }

    #[derive(Debug, Deserialize)]
    #[allow(non_snake_case)]
    struct EagleResponse {
        pub InstantaneousDemand: EagleDemand,
    }

    pub struct EagleClient {
        config: config::Eagle,
    }

    impl EagleClient {
        pub fn new(config: config::Eagle) -> EagleClient {
            EagleClient { config: config }
        }

        pub fn start_update(&self) {
            let user_header = User(self.config.user.clone()).to_owned();
            let password_header = Password(self.config.password.clone()).to_owned();
            let cloud_id_header = CloudId(self.config.cloud_id.clone()).to_owned();
            let sleep_duration = Duration::new(self.config.update_interval_secs as u64, 0);

            thread::spawn(move || {
                let client = reqwest::Client::new();
                loop {
                    let mut resp = client
                        .post("https://rainforestcloud.com:9445/cgi-bin/post_manager")
                        .header(user_header.clone())
                        .header(password_header.clone())
                        .header(cloud_id_header.clone())
                        .body(
                            "<Command><Name>get_instantaneous_demand</Name><Format>JSON</Format></Command>",)
                        .send()
                        .unwrap();
                    let resp: EagleResponse = resp.json().unwrap();
                    INSTANT_POWER.set(resp.InstantaneousDemand.get_power().unwrap());
                    thread::sleep(sleep_duration);
                }
            });
        }
    }
}

struct MetricsService {
    encoder: TextEncoder,
    eagle_client: client::EagleClient,
}

impl MetricsService {
    pub fn new(eagle_client: client::EagleClient) -> MetricsService {
        eagle_client.start_update();

        MetricsService {
            encoder: TextEncoder::new(),
            eagle_client: eagle_client,
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

    let matches = App::new("Prom Eagle")
        .version("0.1.0")
        .author("Hyrum Wright <hyrum@hyrumwright.org>")
        .about("Exports power usage to Prometheus from an Eagle power monitor")
        .arg(
            Arg::with_name("config")
                .long("config")
                .help("File to use for configuration")
                .default_value("config.yml")
                .takes_value(true),
        )
        .get_matches();

    let config = matches.value_of("config").unwrap();
    let config = config::Config::new(config).unwrap();

    let addr = "0.0.0.0".parse().unwrap();
    let addr = net::SocketAddr::new(addr, config.server.port);
    info!("Starting server for {}", addr);
    let server = Http::new()
        .bind(&addr, move || {
            Ok(MetricsService::new(client::EagleClient::new(
                config.eagle.clone(),
            )))
        })
        .unwrap();
    server.run().unwrap();
}
