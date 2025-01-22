
use rand::{thread_rng, Rng};
use tokio::{
    io::{self, AsyncBufReadExt,AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::{broadcast, mpsc, oneshot},
};

type ClientId = u16;

/// Message representing a basic message that's either received
/// by the client or sent from the server.
#[derive(Debug, Clone)]
struct Message(ClientId, String);

impl ToString for Message {
    fn to_string(&self) -> String {
        format!("MESSAGE:{} {}\n", self.0, self.1).to_string()
    }
}

/// OutgoingMessage represents a message that's sent to the client.
#[derive(Debug, Clone)]
enum OutgoingMessage {
    Message(Message),
    Login(ClientId),
    Ack,
}

/// ServerAction represents an action that's taken by the server.
/// Currently we only support broadcasting messages but in the future
/// we may support requesting replays, deletions or announcing logouts. 
#[derive(Debug)]
enum ServerAction {
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

struct ClientConnection {
    socket: TcpStream,
    client_id: ClientId,
    chatroom: mpsc::Sender<ServerAction>,
    consumer: broadcast::Receiver<Message>,
}

impl ClientConnection {
    fn new(
        socket: TcpStream,
        chatroom: mpsc::Sender<ServerAction>,
        consumer: broadcast::Receiver<Message>,
        client_id: ClientId,
    ) -> Self {
        Self {
            socket,
            client_id,
            chatroom,
            consumer,
        }
    }

    async fn drain_client_msg<R: AsyncBufReadExt + Unpin>(
        client_id: ClientId, 
        reader: &mut R
    ) -> Result<Option<Message>, io::Error> {
        // Read just one line at a time
        if let Some(line) = reader.lines().next_line().await? {
            if !line.trim().is_empty() {
                return Ok(Some(Message(client_id, line)));
            }
        }
        Ok(None)
    }

    /// Send a message to the client.
    async fn send_message<W: AsyncWriteExt + Unpin>(writer: &mut W, msg: OutgoingMessage) -> Result<(), io::Error> {
        let msg_str = msg.to_string();
        writer.write_all(msg_str.as_bytes()).await?;
        Ok(())
    }

    /// Replay messages that were missed by this client. Broadcasts all messages
    /// in the chatroom.
    async fn replay_messages<W: AsyncWriteExt + Unpin>(chatroom: &mut mpsc::Sender<ServerAction>, writer: &mut W) -> Result<(), io::Error> {
        let (oneshot_producer, oneshot_consumer) = oneshot::channel();
        chatroom.send(ServerAction::ReplayMessages(oneshot_producer)).await.expect("Couldn't send message");

        let messages = oneshot_consumer.await.expect("Couldn't receive messages");

        for msg in messages {
            Self::send_message(writer, msg.into()).await?;
        }

        Ok(())
    }

    /// Main method of ClientConnection. 
    ///
    /// Listens for messages from the client as well
    /// as well as broadcasts messages.
    pub async fn listen(&mut self) -> Result<(), io::Error> {
        // New client connnection has been established by this point.
        println!(
            "connected {} {}",
            self.socket.local_addr().unwrap().ip(),
            self.socket.local_addr().unwrap().port()
        );

        // Readers and Writers
        let (reader, mut writer) = self.socket.split();
        let mut buf_reader = BufReader::new(reader);

        // Send login message
        Self::send_message(&mut writer, OutgoingMessage::Login(self.client_id)).await?;

        // Replay messages to client
        Self::replay_messages(&mut self.chatroom, &mut writer).await?;

        loop {
            // We use select here because we don't want one
            // to block the other.
            // We could repeatedly consume messages from the chatroom but still
            // want to read from the client when we're able.
            // In other words: we need to action on EITHER of these events being READY.
            tokio::select! {
                // Drain chat & send messages to client
                result = self.consumer.recv() => {
                    let msg = result.unwrap();
                    if msg.0 == self.client_id {
                        continue ;
                    }

                    Self::send_message(&mut writer, msg.into()).await?;
                }

                // Read messages from client and send to chatroom for re-broadcast
                result = Self::drain_client_msg(self.client_id, &mut buf_reader) => {
                    match result {
                        Ok(Some(msg)) => {
                            // Send ACK to client
                            Self::send_message(&mut writer, OutgoingMessage::Ack).await?;

                            // Send message to chatroom
                            self.chatroom.send(msg.into()).await.expect("Couldn't send message");
                        }
                        Ok(None) => (), // No message from client
                        Err(_e) => {
                            // Client disconnected
                            break;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

struct Server {}

impl Server {
    async fn start_server(addr: &str) {
        let (client_msg_producer, client_msg_receiver) = mpsc::channel(256);
        let (chat_msg_producer, _chat_msg_receiver) = broadcast::channel(256);

        let mut chat = Chat::new(client_msg_receiver, chat_msg_producer.clone());

        // Chat manager should be running in the background
        let _chat_task = tokio::spawn(async move {
            chat.run().await;
        });

        // Now we need to listen for clients
        let listener = TcpListener::bind(addr)
            .await
            .expect("Couldn't establish connection to socket");

        println!(
            "listening on port {}",
            listener.local_addr().unwrap().port()
        );

        let mut user_id = thread_rng().gen_range(1000..5000);

        loop {
            let (socket, _) = listener
                .accept()
                .await
                .expect("Couldn't accept connection.");

            let client_producer = client_msg_producer.clone();
            let chat_receiver = chat_msg_producer.subscribe();

            // Get and increment client id
            let client_user_id = user_id;
            user_id += 1;

            // Create client connection
            tokio::spawn(async move {
                ClientConnection::new(socket, client_producer, chat_receiver, client_user_id)
                    .listen()
                    .await
            });
        }
    }
}

struct Chat {
    chat: Vec<Message>,
    receiver: mpsc::Receiver<ServerAction>,
    broadcaster: broadcast::Sender<Message>,
}

impl Chat {
    fn new(receiver: mpsc::Receiver<ServerAction>, broadcaster: broadcast::Sender<Message>) -> Self {
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

    async fn run(&mut self) {
        loop {
            if let Some(action) = self.receiver.recv().await {
                self.handle_action(action);
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let addr = "127.0.0.1:8888";

    Server::start_server(addr).await;
}
