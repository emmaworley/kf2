use clap::{Parser, Subcommand};
use kf2_proto::kf2::session_service_client::SessionServiceClient;
use kf2_proto::kf2::{
    CreateSessionRequest, DeleteSessionRequest, GetSessionRequest, ListSessionsRequest,
};

#[derive(Parser)]
#[command(name = "kf2ctl", version, about = "KF2 CLI")]
struct Cli {
    /// gRPC server address
    #[arg(long, default_value = "http://127.0.0.1:3000", global = true)]
    server: String,

    #[command(subcommand)]
    command: Resource,
}

#[derive(Subcommand)]
enum Resource {
    /// Manage sessions
    Session {
        #[command(subcommand)]
        action: SessionAction,
    },
}

#[derive(Subcommand)]
enum SessionAction {
    /// Create a new session
    Create,
    /// List all sessions
    List,
    /// Get a session by ID
    Get {
        /// Session ID
        id: String,
    },
    /// Delete a session by ID
    Delete {
        /// Session ID
        id: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let mut client = SessionServiceClient::connect(cli.server).await?;

    match cli.command {
        Resource::Session { action } => match action {
            SessionAction::Create => {
                let resp = client.create_session(CreateSessionRequest {}).await?;
                let session = resp.into_inner().session.unwrap();
                println!("{}", session.id);
            }
            SessionAction::List => {
                let resp = client.list_sessions(ListSessionsRequest {}).await?;
                for s in resp.into_inner().sessions {
                    println!("{}\t{}\t{}", s.id, s.created_at, s.updated_at);
                }
            }
            SessionAction::Get { id } => {
                let resp = client.get_session(GetSessionRequest { id }).await?;
                let s = resp.into_inner().session.unwrap();
                println!("{}\t{}\t{}", s.id, s.created_at, s.updated_at);
            }
            SessionAction::Delete { id } => {
                let resp = client.delete_session(DeleteSessionRequest { id }).await?;
                if resp.into_inner().deleted {
                    println!("deleted");
                } else {
                    println!("not found");
                }
            }
        },
    }

    Ok(())
}
