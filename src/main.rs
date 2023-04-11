use futures_util::{SinkExt, StreamExt, TryFutureExt};
use quick_replace_web::{user, Files, Users};
use serde_json::{json, Value};
use std::{
    env,
    fs::{self},
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};
use tokio_stream::wrappers::UnboundedReceiverStream;
use warp::{
    ws::{Message, WebSocket},
    Filter,
};

use quick_replace::worker;

use tokio::sync::mpsc::{self};

static NEXT_USER_ID: AtomicUsize = AtomicUsize::new(1);

#[tokio::main]
async fn main() {
    let users = Users::default();
    let users = warp::any().map(move || users.clone());

    let files = Files::default();
    let files = warp::any().map(move || files.clone());

    let websocket_upgrade = warp::path("ws").and(warp::ws()).and(users).and(files).map(
        |ws: warp::ws::Ws, users, files| {
            ws.on_upgrade(move |socket| user_connected(socket, users, files))
        },
    );

    // TODO: 404.
    let index = warp::path::end()
        .map(|| warp::reply::html(fs::read_to_string("./static/index.html").unwrap()));

    let files = warp::path("temp").and(warp::fs::dir("./temp/"));

    let routes = index.or(websocket_upgrade).or(files);

    fn get_server_port() -> u16 {
        env::var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8080)
    }

    warp::serve(routes)
        .run(([0, 0, 0, 0], get_server_port()))
        .await;
}

async fn user_connected(ws: WebSocket, users: Users, files: Files) {
    let user_id = NEXT_USER_ID.fetch_add(1, Ordering::Relaxed);

    let files_copy = files.clone();

    let (mut user_tx, mut user_rx) = ws.split();
    let (tx, rx) = mpsc::unbounded_channel::<Message>();
    let mut rx = UnboundedReceiverStream::new(rx);

    user::connect(&mut user_tx).await;

    tokio::task::spawn(async move {
        while let Some(message) = rx.next().await {
            let parsed_json: Value = serde_json::from_str(message.to_str().unwrap()).unwrap();

            let msg = if parsed_json["isOnlyMatches"] == false {
                let data = parsed_json["data"]["data"].as_str().unwrap();
                let from = parsed_json["data"]["from"].as_str().unwrap();
                let to = parsed_json["data"]["to"].as_str().unwrap();
                let filename = parsed_json["data"]["filename"].as_str().unwrap();
                let result = data.replace(&from, &to);

                //  Create new file.
                let filepath = format!("./temp/{}", filename);
                if !Path::new("./temp").exists() {
                    fs::create_dir("./temp")
                        .unwrap_or_else(|e| println!("Error creating dir: {}", e));
                }

                worker::create_file_and_put_contents(result, &filepath).unwrap();

                // Write user_id and filepath, used for file deletion on user discconnect.
                let user_id = parsed_json["user_id"].as_i64().unwrap();
                let user_id = usize::try_from(user_id).unwrap();
                files.write().await.insert(user_id, filepath);

                let json = json!({
                    "isOnlyMatches": false,
                    "matches": parsed_json["matches"],
                    "filepath": filename,
                });

                Message::text(json.to_string())
            } else {
                Message::text(message.to_str().unwrap())
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
            Ok(msg) => user::request(user_id, msg, &users).await,
            Err(e) => {
                eprintln!("websocket error(uid={}): {}", user_id, e);
                break;
            }
        };
    }

    user::disconnected(user_id, &users, files_copy).await;
}
