mod db_con;

use crate::db_con::get_db_con;

use tokio::net::TcpListener;
use tokio_websockets::{ServerBuilder, Message};
use futures_util::{StreamExt, SinkExt};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use sqlx::{postgres::PgPoolOptions, types::time::{self, OffsetDateTime}, PgPool};
use ::time::{format_description::well_known::Rfc3339, macros::format_description};
use jsonwebtoken::{encode, decode, Header, EncodingKey, DecodingKey, Validation};
use bcrypt::{hash, verify, DEFAULT_COST};
use axum::{
    routing::{post, get},
    Router, Json, Extension, http::StatusCode, extract::State,
};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tower_http::cors::{CorsLayer, Any};
use hyper::header;
use serde_json;

#[derive(Serialize, Deserialize)]
struct User {
    username: String,
    password: String,
}

#[derive(Serialize, Deserialize)]
struct LoginResponse {
    token: String,
}

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct ChatMessage {
    username: String,
    content: String,
    timestamp: String,
}

struct AppState {
    pool: PgPool,
    jwt_secret: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&get_db_con())
        .await?;

    let jwt_secret = "your-secret-key".to_string();
    let state = Arc::new(AppState { pool, jwt_secret });

    // Configure CORS
    let cors = CorsLayer::new()
        .allow_origin("http://127.0.0.1:5050".parse::<header::HeaderValue>().unwrap()) // frontend origin
        .allow_headers(vec![header::AUTHORIZATION, header::CONTENT_TYPE]) // specific headers
        .allow_methods(vec![axum::http::Method::GET, axum::http::Method::POST]); // specific HTTP methods


    // HTTP API for registration and login
    let app = Router::new()
        .route("/register", post(register_handler))
        .route("/login", post(login_handler))
        .route("/messages", post(get_messages_handler))
        .with_state(state.clone())
        .layer(cors); // CORS layer

    // HTTP server
    let _http_server = tokio::spawn(async {
        axum_server::bind("127.0.0.1:3000".parse().unwrap())
            .serve(app.into_make_service())
            .await
            .unwrap();
    });

    // WebSocket server
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
        println!("WebSocket server listening on 127.0.0.1:8080");

    let (broadcast_tx, _) = broadcast::channel::<String>(100);
    let broadcast_tx = Arc::new(broadcast_tx);

    while let Ok((stream, addr)) = listener.accept().await {
        println!("New client connected: {}", addr);
        let tx = broadcast_tx.clone();
        let pool = state.pool.clone();

        tokio::spawn(async move {
            if let Err(e) = handle_connection_with_cors(stream, tx, pool).await {
                eprintln!("Error handling connection: {}", e);
            }
        });
    }

    Ok(())
}


async fn handle_connection_with_cors(
    stream: tokio::net::TcpStream,
    broadcast_tx: Arc<broadcast::Sender<String>>,
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut ws_stream = ServerBuilder::new()
        .accept(stream)
        .await
        .map_err(|e| {
            eprintln!("WebSocket upgrade failed: {}", e);
            e
        })?;

    let mut broadcast_rx = broadcast_tx.subscribe();

    loop {
        tokio::select! {
            msg = ws_stream.next() => {
                match msg {
                    Some(Ok(message)) => {
                        if let Some(text) = message.as_text() {
                            if let Ok(chat_message) = serde_json::from_str::<ChatMessage>(text) {
                                println!("Broadcasting message from {}: {}", 
                                    chat_message.username, chat_message.content);

                                    let parsed_timestamp = OffsetDateTime::parse(&chat_message.timestamp, &Rfc3339);
                                
                                    match parsed_timestamp {
                                        Ok(valid_timestamp) => {
                                            sqlx::query!(
                                                "INSERT INTO messages (username, content, timestamp) VALUES ($1, $2, $3)",
                                                chat_message.username,
                                                chat_message.content,
                                                valid_timestamp
                                            )
                                            .execute(&pool)
                                            .await?;
                                            
                                            let _ = broadcast_tx.send(text.to_string());
                                        }
                                        Err(e) => {
                                            eprintln!("Invalid timestamp format: {}", e);
                                        }
                                    }
                            }
                        }
                    }
                    Some(Err(e)) => {
                        eprintln!("WebSocket error: {}", e);
                        break;
                    }
                    None => break, // Client disconnected
                }
            }
            
            Ok(msg) = broadcast_rx.recv() => {
                if let Err(e) = ws_stream.send(Message::text(msg)).await {
                    eprintln!("Error sending message: {}", e);
                    break;
                }
            }
        }
    }

    println!("Client disconnected");
    Ok(())
}

async fn register_handler(
    State(state): State<Arc<AppState>>,
    Json(user): Json<User>,
) -> Result<StatusCode, (StatusCode, String)> {
    // Check if username already exists
    let existing_user = sqlx::query!("SELECT username FROM users WHERE username = $1", user.username)
        .fetch_optional(&state.pool)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string()))?;

    if existing_user.is_some() {
        return Err((StatusCode::BAD_REQUEST, "Username already exists".to_string()));
    }

    // Hash password
    let hashed_password = hash(user.password.as_bytes(), DEFAULT_COST)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Password hashing error".to_string()))?;

    sqlx::query!(
        "INSERT INTO users (username, password_hash) VALUES ($1, $2)",
        user.username,
        hashed_password
    )
    .execute(&state.pool)
    .await
    .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string()))?;

    Ok(StatusCode::CREATED)
}

async fn login_handler(
    State(state): State<Arc<AppState>>,
    Json(user): Json<User>,
) -> Result<Json<LoginResponse>, (StatusCode, String)> {
    let db_user = sqlx::query!(
        "SELECT username, password_hash FROM users WHERE username = $1",
        user.username
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string()))?
    .ok_or((StatusCode::UNAUTHORIZED, "Invalid credentials".to_string()))?;

    // Verify password
    let valid = verify(user.password.as_bytes(), &db_user.password_hash)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Password verification error".to_string()))?;

    if !valid {
        return Err((StatusCode::UNAUTHORIZED, "Invalid credentials".to_string()));
    }

    let expiration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as usize + 24 * 3600; // 24 hours

    let claims = Claims {
        sub: user.username,
        exp: expiration,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.jwt_secret.as_bytes()),
    )
    .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Token generation error".to_string()))?;

    Ok(Json(LoginResponse { token }))
}

async fn get_messages_handler(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ChatMessage>>, (StatusCode, String)> {
    let messages = sqlx::query!(
        "SELECT username, content, timestamp FROM messages ORDER BY timestamp ASC LIMIT 100"
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string()))?;

    if messages.is_empty() {
        eprintln!("No messages found in the database.");
    }

    let messages: Vec<ChatMessage> = messages
        .into_iter()
        .map(|msg| ChatMessage {
            username: msg.username,
            content: msg.content,
            timestamp: msg.timestamp.to_string(),
        })
        .collect();

    println!("Retrieved {} messages.", messages.len());

    Ok(Json(messages))
}
