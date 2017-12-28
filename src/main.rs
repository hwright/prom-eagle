// A simple app to read values from the rainforest cloud and export them as
// prometheus metrics

extern crate clap;
use clap::{Arg, App};

#[macro_use]
extern crate log;
extern crate env_logger;

fn main() {
    env_logger::init().unwrap();

    let matches = App::new("Prom Rain")
        .version("0.1.0")
        .author("Hyrum Wright <hyrum@hyrumwright.org>")
        .arg(Arg::with_name("port")
             .short("p")
             .long("port")
             .help("Which port to export metrics on")
             .default_value("9090")
             .takes_value(true))
        .get_matches();

    let port = matches.value_of("port").unwrap();
    let port: u16 = port.parse().expect("Port value is not a valid integer");

    info!("Port: {}", port);
}
