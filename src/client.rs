use tokio::{io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader}, net::TcpStream, sync::{broadcast, mpsc, oneshot}};

use crate::types::{ClientId, Message, OutgoingMessage, ServerAction};

pub struct ClientConnection {
    socket: TcpStream,
    client_id: ClientId,
    chatroom: mpsc::Sender<ServerAction>,
    consumer: broadcast::Receiver<Message>,
}

impl ClientConnection {
    pub fn new(
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