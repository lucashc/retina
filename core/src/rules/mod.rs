//! This module is the core rule loader for the Regex filter.
//! It operates by creating a Unix socket over which JSON-coded rules can be sent.
//! Upon successful receive, it compiles the regulex expressions into a `RegexSet`-object which is a compact and efficient representation of the regexes as a DFA.
//! The JSON rule format looks like:
//! ```json
//! {
//!     "rules": [
//!         "[a-z0-9.]+@[a-z0-9.-]+",
//!         "sensitive"
//!     ]
//! }
//! ```

use regex::bytes::RegexSet;
use serde::{Deserialize, Serialize};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use serde_json::Deserializer;

/// Structure that holds the Unix socket to receive rules in JSON format and it holds `Arc`-references to each core-local `RegexSet` object to update it sequentially upon request.
pub struct Rules {
    unix_socket: UnixListener,
    regexsets_from_cores: Vec<Arc<RwLock<RegexSet>>>,
}

impl Rules {
    /// This creates a new `Rules` structure by specifying a socket path and the vector of references to the regexes per core.
    pub fn new(socket_path: PathBuf, regexsets_from_cores: Vec<Arc<RwLock<RegexSet>>>) -> Rules {
        Rules {
            unix_socket: UnixListener::bind(socket_path).unwrap(),
            regexsets_from_cores,
        }
    }

    /// This function must be spawned in a new thread and handles the listening on the Unix socket.
    /// Upon exit of the program, the Unix Socket gets removed, which caused the loop to stop.
    pub fn rule_update_loop(&self) {
        for stream in self.unix_socket.incoming() {
            log::info!("Accepted connection on socket");
            match stream {
                Ok(stream) => {
                    self.handle_connection(stream);
                }
                Err(err) => {
                    log::warn!("Rule daemon: Connection failed {:?}", err);
                }
            }
        }
    }

    fn handle_connection(&self, stream: UnixStream) {
        let serde_stream = Deserializer::from_reader(stream).into_iter::<RuleFormat>();
        for rule_object in serde_stream {
            match rule_object {
                Ok(rule_object) => {
                    let new_regexset = RegexSet::new(rule_object.rules);
                    match new_regexset {
                        Ok(new_regexset) => {
                            log::info!("Received proper rule set, doing update");
                            self.update_rules(new_regexset);
                        }
                        Err(err) => {
                            log::warn!("Rule daemon: Issue compiling regexes: {:?}", err);
                        }
                    }
                }
                Err(err) => {
                    log::warn!("Rule daemon: Invalid JSON read: {:?}", err);
                }
            }
        }
    }

    fn update_rules(&self, new_regexset: RegexSet) {
        for existing_regexset in self.regexsets_from_cores.iter() {
            let mut unlocked_regex = existing_regexset.write().unwrap();
            *unlocked_regex = new_regexset.clone();
        }
    }
}

#[derive(Serialize, Deserialize)]
struct RuleFormat {
    rules: Vec<String>,
}
