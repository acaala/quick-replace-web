use futures_util::{SinkExt, StreamExt, TryFutureExt};
use serde_json::{json, Value};
use std::{
    borrow::Cow,
    collections::HashMap,
    fs,
    path::Path,
    process,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tokio_stream::wrappers::UnboundedReceiverStream;
use warp::{
    ws::{Message, WebSocket},
    Filter,
};

use quick_replace::worker;

use tokio::sync::{
    mpsc::{self},
    RwLock,
};

static NEXT_USER_ID: AtomicUsize = AtomicUsize::new(1);

type Users = Arc<RwLock<HashMap<usize, mpsc::UnboundedSender<Message>>>>;

#[tokio::main]
async fn main() {
    let users = Users::default();
    let users = warp::any().map(move || users.clone());

    let websocket_upgrade = warp::path("ws")
        .and(warp::ws())
        .and(users)
        .map(|ws: warp::ws::Ws, users| ws.on_upgrade(move |socket| user_connected(socket, users)));

    // TODO: 404.
    let index = warp::path::end()
        .map(|| warp::reply::html(fs::read_to_string("./static/index.html").unwrap()));

    let files = warp::path("temp").and(warp::fs::dir("./temp/"));

    let routes = index.or(websocket_upgrade).or(files);

    warp::serve(routes).run(([127, 0, 0, 1], 3000)).await;
}

async fn user_connected(ws: WebSocket, users: Users) {
    let user_id = NEXT_USER_ID.fetch_add(1, Ordering::Relaxed);

    let (mut user_tx, mut user_rx) = ws.split();

    let (tx, rx) = mpsc::unbounded_channel::<Message>();
    let mut rx = UnboundedReceiverStream::new(rx);

    tokio::task::spawn(async move {
        while let Some(message) = rx.next().await {
            let parsed_json: Value = serde_json::from_str(message.to_str().unwrap()).unwrap();

            let msg: Message = if parsed_json["isOnlyMatches"] == true {
                Message::text(message.to_str().unwrap())
            } else {
                let data = parsed_json["data"]["data"].as_str().unwrap();
                let from = parsed_json["data"]["from"].as_str().unwrap();
                let to = parsed_json["data"]["to"].as_str().unwrap();
                let filename = parsed_json["data"]["filename"].as_str().unwrap();

                let result = data.replace(&from, &to);

                let filepath = format!("./temp/{}", filename);
                worker::create_file_and_put_contents(result, &filepath).unwrap();

                let json = json!({
                    "isOnlyMatches": false,
                    "matches": parsed_json["matches"],
                    "filepath": filename,
                });

                Message::text(json.to_string())
            };

            user_tx
                .send(msg)
                .unwrap_or_else(|e| eprintln!("Websocket err: {}", e))
                .await;
        }
    });

    users.write().await.insert(user_id, tx);

    while let Some(result) = user_rx.next().await {
        match result {
            Ok(msg) => user_request(user_id, msg, &users).await,
            Err(e) => {
                eprintln!("websocket error(uid={}): {}", user_id, e);
                break;
            }
        };
    }

    user_disconnected(user_id, &users).await;
}

async fn user_request(user_id: usize, msg: Message, users: &Users) {
    let msg = if let Ok(s) = msg.to_str() {
        s
    } else {
        return;
    };

    let parsed_user_req: Value = serde_json::from_str(&msg).unwrap_or_else(|e| {
        println!("Error parsing json: {}", e);
        process::exit(0)
    });

    let from = &parsed_user_req["from"].as_str().unwrap().to_owned();
    let data = &parsed_user_req["data"].as_str().unwrap().to_owned();

    let str_matches = data.matches(from).into_iter().count();

    let match_json = json!({
        "isOnlyMatches": true,
        "matches": str_matches
    });

    let user_req_json = json!({
        "isOnlyMatches": false,
        "matches": str_matches,
        "data": parsed_user_req,
    });

    for (&uid, tx) in users.read().await.iter() {
        if user_id == uid {
            if let Err(_error) = tx.send(Message::text(&user_req_json.to_string())) {};
        } else {
            if let Err(_error) = tx.send(Message::text(&match_json.to_string())) {};
        }
    }
}

async fn user_disconnected(user_id: usize, users: &Users) {
    eprintln!("User {} disconnected", user_id);

    users.write().await.remove(&user_id);
}
