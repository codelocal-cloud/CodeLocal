use std::{env, net::SocketAddr};

use codelocal_backend::router;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "codelocal_backend=info,tower_http=info".into()),
        )
        .init();

    let host = env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1".to_owned());
    let port = env::var("PORT")
        .unwrap_or_else(|_| "8080".to_owned())
        .parse::<u16>()?;
    let address: SocketAddr = format!("{host}:{port}").parse()?;
    let listener = TcpListener::bind(address).await?;
    tracing::info!(address = %listener.local_addr()?, "CodeLocal beta API listening");
    axum::serve(listener, router()).await?;
    Ok(())
}
