use std::{
    collections::{HashMap, HashSet},
    sync::atomic::AtomicUsize,
};

use axum::{
    body::Body,
    extract::{Path, State},
    http::Request,
    response::IntoResponse,
    routing::get,
    Router,
};
use tokio::sync::mpsc::{self, Receiver, Sender};

static ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct RaffleId(String);
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct UserId(String);

enum Message {
    NewRaffle(RaffleId),
    JoinRaffle {
        raffle_id: RaffleId,
        user_id: UserId,
    },
}

async fn operator(mut channel: Receiver<Message>) {
    let mut raffles = HashMap::new();

    while let Some(msg) = channel.recv().await {
        match msg {
            Message::NewRaffle(id) => {
                let (tx, rx) = mpsc::channel(128);
                raffles.insert(id, tx);
                tokio::spawn(handle_raffle(rx));
            }
            Message::JoinRaffle { raffle_id, user_id } => {
                if let Some(raffle) = raffles.get(&raffle_id) {
                    raffle.send(user_id).await.unwrap();
                }
            }
        }
    }
}

async fn handle_raffle(mut channel: Receiver<UserId>) {
    let mut users = HashSet::new();
    while let Some(user) = channel.recv().await {
        println!("{}", user.0);
        users.insert(user);
    }
}

async fn hello_world() -> &'static str {
    "Hello, world!"
}

async fn new(State(ch): State<Sender<Message>>) -> impl IntoResponse {
    let id = cool_id_generator::get_id(cool_id_generator::Size::Medium);
    let id = RaffleId(id);
    let redirect = format!("https://flottery.shuttleapp.rs/join/{}", id.0);
    ch.send(Message::NewRaffle(id)).await.unwrap();
    let code = qrcode::QrCode::new(redirect).unwrap();
    let svg = code.render::<qrcode::render::svg::Color>().build();
    let mut response = svg.into_response();
    response
        .headers_mut()
        .insert("content-type", "image/svg+xml".parse().unwrap());
    response
}

async fn join(
    Path(raffle): Path<String>,
    State(ch): State<Sender<Message>>,
    req: Request<Body>,
) -> String {
    let id = remote_addr(&req).unwrap().to_owned();
    let message = format!("You joined raffle {id}");

    ch.send(Message::JoinRaffle {
        raffle_id: RaffleId(raffle),
        user_id: UserId(id),
    })
    .await
    .unwrap();

    message
}

fn remote_addr(req: &Request<Body>) -> Option<&str> {
    let headers = req.headers();
    headers
        .get("Forwarded")
        .or(headers.get("X-Forwarded-For"))
        .or(headers.get("Host"))
        .map(|header| header.to_str().unwrap())
}

#[shuttle_runtime::main]
async fn main() -> shuttle_axum::ShuttleAxum {
    let (op, rx) = mpsc::channel(512);
    tokio::spawn(operator(rx));
    let router = Router::new()
        .route("/", get(hello_world))
        .route("/new", get(new))
        .route("/join/:id", get(join))
        .with_state(op);

    Ok(router.into())
}
