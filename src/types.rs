use tokio::sync::oneshot;

pub type ClientId = u16;

/// Message representing a basic message that's either received
/// by the client or sent from the server.
#[derive(Debug, Clone)]
pub struct Message(pub ClientId, pub String);

impl ToString for Message {
    fn to_string(&self) -> String {
        format!("MESSAGE:{} {}\n", self.0, self.1).to_string()
    }
}

/// OutgoingMessage represents a message that's sent to the client.
#[derive(Debug, Clone)]
pub enum OutgoingMessage {
    Message(Message),
    Login(ClientId),
    Ack,
}

/// ServerAction represents an action that's taken by the server.
/// Currently we only support broadcasting messages but in the future
/// we may support requesting replays, deletions or announcing logouts. 
#[derive(Debug)]
pub enum ServerAction {
    BroadcastMessage(Message),
    // There's likely a better way than to send a vector of messages
    // but this is a simple solution for now.
    ReplayMessages(oneshot::Sender<Vec<Message>>),
    // TODO: Other actions. Perhaps announce logouts
}

impl ToString for OutgoingMessage {
    fn to_string(&self) -> String {
        match self {
            OutgoingMessage::Message(msg) => msg.to_string(),
            OutgoingMessage::Login(client_id) => format!("LOGIN:{client_id}\n"),
            OutgoingMessage::Ack => "ACK:MESSAGE\n".to_string(),
        }
    }
}

impl Into<OutgoingMessage> for Message {
    fn into(self) -> OutgoingMessage {
        OutgoingMessage::Message(self)
    }
}

impl Into<ServerAction> for Message {
    fn into(self) -> ServerAction {
        ServerAction::BroadcastMessage(self)
    }
}