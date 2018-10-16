use std::time::Instant;

use std::sync::mpsc::Sender;
use std::sync::mpsc;

use std::sync::Arc;

use std::thread;

use irc::client::prelude::{IrcClient, Client, ClientExt, Command};
use irc::proto::message::Message;
use irc::proto::ChannelExt;

#[derive(Debug)]
pub struct Carlo {
    start_time: Instant,
    client: Arc<IrcClient>
}


#[derive(Debug)]
enum Event {
    IncomingIrcMessage(Message),
}


impl Carlo {
    pub fn new() -> Carlo {
        Carlo {
            start_time: Instant::now(),
            client: Arc::new(IrcClient::new("config.toml").unwrap())
        }
    }

    pub fn run(&mut self) {
        let (tx, rx) = mpsc::channel();

        self.client.identify().unwrap();

        let listener = Listener::new(self.client.clone(), tx);
        let hlisten = thread::spawn(move || { listener.listen() });

        for event in rx.iter() {
            if let Some(message) = self.handle(&event) {
                self.client.send(message).unwrap();
            }
        }
        hlisten.join().unwrap();
    }

    fn handle(&self, event: &Event) -> Option<Message> {
        match event {
            Event::IncomingIrcMessage(message) => self.handle_irc(message),
        }
    }

    fn handle_irc(&self, message: &Message) -> Option<Message> {
        let cmd_prefix = self.client.current_nickname().to_string();
        match &message.command {
            Command::PRIVMSG(channel, msg) => {
                if !channel.is_channel_name() || msg.trim_left().starts_with(&cmd_prefix) {
                    let reply_txt = self.process_msg(&msg);
                    let cmd = Command::PRIVMSG(message.response_target().unwrap().to_string(), reply_txt);
                    let reply = Message::from(cmd);
                    Some(reply)
                } else {
                    None
                }
            },
            _ => None
        }
    }

    fn process_msg(&self, msg: &str) -> String {
        if msg.contains("uptime") {
            format!("uptime = {} seconds",
                    self.start_time.elapsed().as_secs())
        } else {
            "beep boop".to_string()
        }
    }
}



#[derive(Debug)]
struct Listener {
    client: Arc<IrcClient>,
    tx: Sender<Event>,
}

impl Listener {
    fn new(client: Arc<IrcClient>, tx: Sender<Event>) -> Listener {
        Listener { client, tx }
    }

    fn listen(&self) {
        self.client.for_each_incoming(|irc_msg| {
            self.tx.send(Event::IncomingIrcMessage(irc_msg)).unwrap();
        }).unwrap();
    }
}
