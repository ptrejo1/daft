use serde::{Deserialize};
use std::fmt::{Display, Formatter};

#[derive(Debug, Deserialize, Clone)]
pub struct Address {
    pub host: String,
    pub port: usize,
}

impl Display for Address {

    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.host, self.port)
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub address: Address,
    pub nodes: Vec<Address>,
    pub heartbeat_interval: u64,
}
