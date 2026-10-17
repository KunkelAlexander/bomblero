// Run the relay: `cargo run -p server` (listens on port 9001, or $PORT, which
// is how hosting services such as Render tell a container which port to use).

#[tokio::main]
async fn main() {
    let port = std::env::var("PORT").unwrap_or_else(|_| "9001".to_string());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .expect("could not bind the port");
    println!("relay listening on ws://0.0.0.0:{port}/ws");
    server::serve(listener).await;
}
