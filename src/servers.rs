use serde::Serialize;

#[derive(Serialize)]
pub struct Server {
    pub name: &'static str,
    pub port: u16,
}

pub const SERVERS: [Server; 5] = [
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
];

pub fn server_by_name(name: &str) -> Option<&'static Server> {
    SERVERS.iter().find(|s| s.name == name)
}
