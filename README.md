# Rust Chatroom Coding assignment

## Assignment
Create a chatroom server in Rust that supports multiple clients and broadcasts messages to each.

The assignment was to code this in Rust with `tokio` with an optional addon for a single threaded implementation.

## Usage
```usage
cargo run --bin server
```

Alternatively

```usage
./server.sh
```

## Client
To communicate with the server / act as a member in the chat, we can use `nc` via:

```
nc localhost 8888
LOGIN:2294
what<enter>
ACK:MESSAGE
ok<enter>
ACK:MESSAGE
```

## Design Decisions
Overall the design is quite straight forward. A simple chatroom + client (async) threads. When a client connects we assign them a random incrementing identifier. This was a decision I made based off the prompt which did not specify how the identifiers are assigned to clients.