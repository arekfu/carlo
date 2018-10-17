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
        debug!("New Carlo instance");
        Carlo {
            start_time: Instant::now(),
            client: Arc::new(IrcClient::new("config.toml").unwrap())
        }
    }

    pub fn run(&mut self) {
        let (tx, rx) = mpsc::channel();

        debug!("Identifying with server");
        self.client.identify().unwrap();

        let listener = Listener::new(self.client.clone(), tx);
        let hlisten = thread::spawn(move || { listener.listen() });

        for event in rx.iter() {
            if let Some(message) = self.handle(&event) {
                info!("Sending {}", message);
                self.client.send(message).unwrap();
            }
        }
        hlisten.join().unwrap();
    }

    fn handle(&self, event: &Event) -> Option<Message> {
        debug!("Handling event {:?}", event);
        match event {
            Event::IncomingIrcMessage(message) => self.handle_irc(message),
        }
    }

    fn handle_irc(&self, message: &Message) -> Option<Message> {
        debug!("Handling Irc message {:?}", message);
        let cmd_prefix = self.client.current_nickname().to_string();
        match &message.command {
            Command::PRIVMSG(channel, msg) => {
                if !channel.is_channel_name() || msg.trim_left().starts_with(&cmd_prefix) {
                    let reply_to = message.response_target().unwrap().to_string();
                    let source_nick = message.source_nickname().unwrap_or("");
                    self.process_msg(&source_nick, &reply_to, &msg)
                } else {
                    None
                }
            },
            _ => None
        }
    }

    fn process_msg(&self, source_nick: &str, reply_to: &str, incoming: &str) -> Option<Message> {
        if incoming.contains("uptime") {
            info!("\"uptime\" command received from {} on {}", source_nick, reply_to);
            let reply = format!("uptime = {} seconds", self.start_time.elapsed().as_secs());
            let cmd = Command::PRIVMSG(reply_to.to_string(), reply);
            Some(Message::from(cmd))
        } else if incoming.starts_with("say ") {
            info!("\"say\" command received from {} on {}", source_nick, reply_to);
            if !self.client.config().is_owner(source_nick) {
                return None;
            }
            let v: Vec<&str> = incoming[4..].trim().splitn(2, ' ').collect();
            if v.len() <= 1 {
                debug!("\"say\" command has no message, not doing anything");
                None
            } else {
                let chan = v[0].to_string();
                let reply = v[1].trim().to_string();
                let cmd = Command::PRIVMSG(chan, reply);
                Some(Message::from(cmd))
            }
        } else {
            debug!("unrecognized command: {}", incoming);
            None
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
            debug!("Listener: sending to master thread: {:?}", irc_msg);
            self.tx.send(Event::IncomingIrcMessage(irc_msg)).unwrap();
        }).unwrap();
    }
}
