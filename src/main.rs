use ws::{CloseCode, Handshake, Message, Result, Sender, Builder};
use url::Url;

use std::env;
use std::net::SocketAddr;
use std::cell::RefCell;
use std::rc::Rc;

use log::{info, warn, debug, log_enabled, Level};

const HELP: &str =
    "This is a debug proxy, which dumps all messages passing through specified port.\n\
    \nSyntax: ws-debug <server-url> <proxy-port>\n\
    \nThe only two parameters are a port number to listen and a websocket url\
    \nto redirect messages to. If a message comes from the <server-url>, it is directed\
    \nto the first client connected to the debug proxy. Looping is forbidden.\n\
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
                debug!("Error: {}", e);
                println!("Websocket URL {} is invalid", arg1);
                std::process::exit(-1);
            });
            let proxy_port = arg2.parse::<u16>().unwrap_or_else(|e| {
                debug!("Error: {}", e);
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
                    client: client.clone()
                }
            } else {
                debug!("Creating handler for a client");
                let mut client = client.borrow_mut();
                if client.is_none() {
                    *client = Some(out);
                }

                Handler::Client {
                    server: server.borrow().as_ref().unwrap().clone()
                }
            }
        })
        .unwrap();

    ws.connect(server_url).unwrap();
    ws.listen(SocketAddr::from(([127,0,0,1], proxy_port))).unwrap();
}

enum Handler {
    Server {
        client: Rc<RefCell<Option<Sender>>>
    },
    Client {
        server: Rc<Sender>
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
            Handler::Server { client } => {
                debug!("Redirecting message from server to client");

                let client = client.borrow_mut();
                assert!(client.is_some());
                client.as_ref().unwrap().send(msg).unwrap()
            },
            Handler::Client { server } => {
                debug!("Redirecting message from client to server");

                server.send(msg).unwrap()
            }
        }
        Ok(())
    }

    fn on_close(&mut self, code: CloseCode, reason: &str) {
        info!("Connection closed: code={:?}, reason=\"{}\"", code, reason)
    }
}