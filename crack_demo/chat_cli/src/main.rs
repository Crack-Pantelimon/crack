use anyhow::Context;
use net_crackpipe::{
    chat::chat_controller::{IChatController, IChatReceiver, IChatSender},
    chat::global_chat::{GlobalChatMessageContent, GlobalChatPresence},
    network_manager::NetworkManager,
    user_identity::UserIdentitySecrets,
};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // init tracing_subscriber from RUST_LOG (default info)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let secrets = UserIdentitySecrets::generate();
    let own_nickname = secrets.user_identity().nickname();
    println!("SELF {}", own_nickname);

    // If this process wins a bootstrap slot, its bootstrap node should also
    // carry the game's extra topics, so use the game network config.
    let network = NetworkManager::init(
        Arc::new(secrets),
        game_logic::network::network_manager_config(),
    )
    .await
    .context("Failed to initialize NetworkManager")?;

    let controller = network
        .global_chat_controller()
        .await
        .context("Failed to get global chat controller")?;

    let sender = controller.sender();
    sender
        .set_presence(&GlobalChatPresence {
            url: "".into(),
            platform: "chat_cli".into(),
            is_server: None,
        })
        .await;

    controller
        .wait_joined(2)
        .await
        .context("Failed wait_joined")?;

    println!("READY {}", own_nickname);

    // Spawn a stdin reader task
    let sender_clone = sender.clone();
    tokio::spawn(async move {
        let stdin = tokio::io::stdin();
        let mut reader = BufReader::new(stdin).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                let msg = GlobalChatMessageContent::TextMessage {
                    text: trimmed.to_string(),
                };
                match sender_clone.broadcast_message(msg).await {
                    Ok(_) => {
                        println!("SENT {}", trimmed);
                    }
                    Err(e) => {
                        eprintln!("Error sending message: {:?}", e);
                    }
                }
            }
        }
    });

    // Main receive loop
    let recv = controller.receiver().await;
    while let Some(msg) = recv.next_message().await {
        let nickname = msg.from.nickname();
        match msg.message {
            GlobalChatMessageContent::TextMessage { text } => {
                println!("RECV {} {}", nickname, text);
            }
            _ => {}
        }
    }

    network.shutdown().await?;
    Ok(())
}
