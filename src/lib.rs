extern crate futures;
extern crate hyper;
extern crate serde;
#[macro_use]
extern crate serde_json;
#[macro_use]
extern crate error_chain;

mod server;

pub use server::JsonServer;
pub use server::{Error, Result, ErrorKind};

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
    }
}
