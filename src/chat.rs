use tokio::sync::{broadcast, mpsc};

use crate::types::{Message, ServerAction};


pub struct Chat {
    chat: Vec<Message>,
    receiver: mpsc::Receiver<ServerAction>,
    broadcaster: broadcast::Sender<Message>,
}

impl Chat {
    pub fn new(receiver: mpsc::Receiver<ServerAction>, broadcaster: broadcast::Sender<Message>) -> Self {
        Self {
            chat: Vec::new(),
            receiver,
            broadcaster,
        }
    }

    fn handle_action(&mut self, action: ServerAction) {
        match action {
            ServerAction::BroadcastMessage(msg) => {
                // Got a message. Save it to chat and distribute
                println!("message {} {}", msg.0, msg.1);

                // We only distribute if it's a message.
                self.chat.push(msg.clone());
                match self.broadcaster.send(msg) {
                    Ok(_) => (),
                    Err(err) => {
                        eprintln!("Error sending message: {err:?}");
                    }
                }
            }
            ServerAction::ReplayMessages(producer) => {
                // Send all messages to the client
                producer.send(self.chat.clone()).unwrap();
            }
        }
    }

    pub async fn run(&mut self) {
        loop {
            if let Some(action) = self.receiver.recv().await {
                self.handle_action(action);
            }
        }
    }
}