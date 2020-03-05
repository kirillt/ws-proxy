use url::Url;
use chrono::Utc;
use serde_json::{Value};
use ws::{CloseCode, Handshake, Message, Result, Sender, Builder};

use std::env;
use std::fs::{File, OpenOptions};
use std::net::SocketAddr;
use std::cell::RefCell;
use std::rc::Rc;

use log::{info, warn, error, debug, log_enabled, Level};
use std::io::Write;

const HELP: &str =
    "This is a debug proxy, which dumps all messages passing through specified port.\n\
    \nSyntax: ws-debug <server-url> <proxy-port> [--pretty-jsons]\n\
    \nThe only two parameters are a port number to listen and a websocket url\
    \nto redirect messages to. If a message comes from the <server-url>, it is directed\
    \nto the last client connected to the debug proxy. Looping is forbidden.\n\
    \nYou can provide --pretty-jsons flag to pretty print jsons when they are encountered.\
    \nThe program will create a separate file for server and client.";

const SERVER_PREFIX: &str = "[server]";

fn main() {
    let mut prettify_json = false;
    let args: Vec<String> = env::args().skip(1)
        .filter(|arg| {
            if arg.as_str() == "--help" {
                println!("{}", HELP);
                std::process::exit(0);
            }
            if arg.as_str() == "--pretty-jsons" {
                prettify_json = true;
                return false;
            }
            return true;
        })
        .collect();

    match args.as_slice() {
        [arg1, arg2] => {
            let server_url = Url::parse(arg1).unwrap_or_else(|e| {
                error!("Error: {}", e);
                println!("Websocket URL {} is invalid", arg1);
                std::process::exit(-1);
            });
            let proxy_port = arg2.parse::<u16>().unwrap_or_else(|e| {
                error!("Error: {}", e);
                println!("Port number {} is invalid", arg2);
                std::process::exit(-1);
            });

            listen(proxy_port, server_url, prettify_json)
        },
        _ => println!("{}", HELP)
    }
}

fn listen(proxy_port: u16, server_url: Url, prettify_json: bool) {
    env_logger::init();
    info!("Listening port {}, redirecting messages to {}", proxy_port, server_url);

    let server: RefCell<Option<Rc<Sender>>> = RefCell::new(None);
    let client: Rc<RefCell<Option<Sender>>> = Rc::new(RefCell::new(None));

    let server_label = server_url.to_string();

    let mut ws = Builder::new()
        .build(|out: Sender| {
            if out.connection_id() == 0 {
                debug!("Creating handler for the server");
                *server.borrow_mut() = Some(Rc::new(out));

                let mut file = provide_file("ws-debug.server.log");
                file.write_fmt(format_args!("{} Proxy connected to the server at {}\n",
                    Utc::now(), server_label)).unwrap();

                Handler::Server {
                    client: client.clone(),
                    log_file: file,
                    prettify_json
                }
            } else {
                debug!("Creating handler for a client");
                let id = out.connection_id();

                let mut client = client.borrow_mut();
                *client = Some(out);

                let mut file = provide_file("ws-debug.client.log");
                file.write_fmt(format_args!("{} Client connected to the proxy with id {}\n",
                    Utc::now(), id)).unwrap();

                Handler::Client {
                    server: server.borrow().as_ref().unwrap().clone(),
                    connection_id: id,
                    log_file: file,
                    prettify_json
                }
            }
        })
        .unwrap();

    ws.connect(server_url).unwrap();
    ws.listen(SocketAddr::from(([127,0,0,1], proxy_port))).unwrap();
}

enum Handler {
    Server {
        client: Rc<RefCell<Option<Sender>>>,
        log_file: File,
        prettify_json: bool,
    },
    Client {
        server: Rc<Sender>,
        connection_id: u32,
        log_file: File,
        prettify_json: bool,
    }
}

impl ws::Handler for Handler {
    fn on_open(&mut self, h: Handshake) -> Result<()> {
        debug!("Connection opened: we are {:?}, they are {:?}", h.local_addr, h.peer_addr);
        if log_enabled!(Level::Warn) && h.peer_addr.is_none() {
            warn!("Connection with unknown address opened");
        }
        Ok(())
    }

    fn on_message(&mut self, msg: Message) -> Result<()> {
        match self {
            Handler::Server { client, log_file, prettify_json } => {
                debug!("Redirecting message from server to client");

                let client = client.borrow_mut();
                assert!(client.is_some());

                client.as_ref().unwrap().send(msg.clone()).unwrap();
                log_to_file(log_file, SERVER_PREFIX, msg, *prettify_json)
            },
            Handler::Client {
                server, connection_id,
                log_file, prettify_json
            } => {
                debug!("Redirecting message from client to server");
                let prefix = format!("[id: {}]", connection_id);

                server.send(msg.clone()).unwrap();
                log_to_file(log_file, &prefix, msg, *prettify_json)
            }
        }
        Ok(())
    }

    fn on_close(&mut self, code: CloseCode, reason: &str) {
        debug!("Connection closed: code={:?}, reason=\"{}\"", code, reason);
    }
}

fn log_to_file(file: &mut File, prefix: &str, msg: Message, prettify_json: bool) {
    let text = pretty_print(msg, prettify_json);
    let result = file.write_fmt(format_args!("{} {} {}",
        Utc::now(), prefix, text));

    result.unwrap_or_else(|e| {
        error!("Error: {}", e);
    })
}

fn pretty_print(msg: Message, prettify_json: bool) -> String {
    match msg {
        Message::Binary(bytes) => {
            debug!("Binary message received while expecting a JSON");
            format!("Binary({:?})", bytes)
        },
        Message::Text(raw) => {
            if prettify_json {
                let value: serde_json::Result<Value> = serde_json::from_str(&raw[..]);

                match value {
                    Ok(value) => {
                        let text = serde_json::to_string_pretty(&value);
                        text.unwrap_or_else(|e| {
                            warn!("Error: {}", e);
                            raw
                        })
                    },
                    Err(e) => {
                        warn!("Error: {}", e);
                        return raw;
                    }
                }
            } else {
                raw
            }
        }
    }
}

//todo: manage resource release
fn provide_file(name: &str) -> File {
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(true)
        .open(name.clone());

    file.unwrap_or_else(|e| {
        error!("Error: {}", e);
        println!("Failed to create file {}", name);
        std::process::exit(-1);
    })
}