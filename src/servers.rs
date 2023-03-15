use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Server {
    pub name: &'static str,
    pub port: u16,
}

pub type Servers = [Server; 6];

pub const SERVERS: Servers = [
    Server {
        name: "bagil",
        port: 2337,
    },
    Server {
        name: "sybil",
        port: 1337,
    },
    Server {
        name: "terry",
        port: 3336,
    },
    Server {
        name: "manuel",
        port: 1447,
    },
    Server {
        name: "campbell",
        port: 6337,
    },
    Server {
        name: "event-hall-us",
        port: 4447,
    },
];

pub fn server_by_name(name: &str) -> Option<&'static Server> {
    SERVERS.iter().find(|s| s.name == name)
}

pub fn server_by_port(port: u16) -> Option<&'static Server> {
    SERVERS.iter().find(|s| s.port == port)
}
