use irc::client::prelude::{Client, IrcClient};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use carlo::Event;

#[derive(Debug)]
pub struct IrcListener {
    client: Arc<IrcClient>,
    tx: Sender<Event>,
}

impl IrcListener {
    pub fn new(client: Arc<IrcClient>, tx: Sender<Event>) -> IrcListener {
        IrcListener { client, tx }
    }

    pub fn listen(&self) {
        self.client
            .for_each_incoming(|irc_msg| {
                debug!("IrcListener: sending to master thread: {:?}", irc_msg);
                self.tx.send(Event::IncomingIrcMessage(irc_msg)).unwrap();
            }).unwrap();
    }
}
