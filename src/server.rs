
use chat::Chat;
use client::ClientConnection;
use rand::{thread_rng, Rng};
use tokio::{
    net::TcpListener,
    sync::{broadcast, mpsc},
};

mod types;
mod client;
mod chat;

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

#[tokio::main]
async fn main() {
    let addr = "127.0.0.1:8888";

    Server::start_server(addr).await;
}
