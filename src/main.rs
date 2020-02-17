use url::Url;
use chrono::Utc;
use ws::{CloseCode, Handshake, Message, Result, Sender, Builder};

use std::env;
use std::fs::File;
use std::net::SocketAddr;
use std::cell::RefCell;
use std::rc::Rc;

use log::{info, warn, error, debug, log_enabled, Level};
use std::io::Write;

const HELP: &str =
    "This is a debug proxy, which dumps all messages passing through specified port.\n\
    \nSyntax: ws-debug <server-url> <proxy-port>\n\
    \nThe only two parameters are a port number to listen and a websocket url\
    \nto redirect messages to. If a message comes from the <server-url>, it is directed\
    \nto the last client connected to the debug proxy. Looping is forbidden.\n\
    \nThe program will create a separate file for each participant.";

fn main() {
    let args: Vec<String> = env::args().skip(1)
        .filter(|arg| {
            if arg.as_str() == "--help" {
                println!("{}", HELP);
                std::process::exit(0);
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
            listen(proxy_port, server_url)
        },
        _ => println!("{}", HELP)
    }
}

fn listen(proxy_port: u16, server_url: Url) {
    env_logger::init();
    info!("Listening port {}, redirecting messages to {}", proxy_port, server_url);

    let server: RefCell<Option<Rc<Sender>>> = RefCell::new(None);
    let client: Rc<RefCell<Option<Sender>>> = Rc::new(RefCell::new(None));

    let mut ws = Builder::new()
        .build(|out: Sender| {
            if out.connection_id() == 0 {
                debug!("Creating handler for the server");
                *server.borrow_mut() = Some(Rc::new(out));

                Handler::Server {
                    client: client.clone(),
                    log_file: provide_file("ws-debug.server.log")
                }
            } else {
                debug!("Creating handler for a client");
                let mut client = client.borrow_mut();
                *client = Some(out);

                Handler::Client {
                    server: server.borrow().as_ref().unwrap().clone(),
                    log_file: provide_file("ws-debug.client.log")
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
        log_file: File
    },
    Client {
        server: Rc<Sender>,
        log_file: File
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
            Handler::Server { client, log_file } => {
                debug!("Redirecting message from server to client");

                let client = client.borrow_mut();
                assert!(client.is_some());

                client.as_ref().unwrap().send(msg.clone()).unwrap();

                log_file
                    .write_fmt(format_args!("{} {:?}\n", Utc::now(), msg))
                    .unwrap();
            },
            Handler::Client { server, log_file } => {
                debug!("Redirecting message from client to server");

                server.send(msg.clone()).unwrap();

                log_file
                    .write_fmt(format_args!("{} {:?}\n", Utc::now(), msg))
                    .unwrap();
            }
        }
        Ok(())
    }

    fn on_close(&mut self, code: CloseCode, reason: &str) {
        debug!("Connection closed: code={:?}, reason=\"{}\"", code, reason);
    }
}

fn provide_file(name: &str) -> File {
    //todo: manage resource release
    let file = File::create(name.clone());
    file.unwrap_or_else(|e| {
        error!("Error: {}", e);
        println!("Failed to create file {}", name);
        std::process::exit(-1);
    })
}