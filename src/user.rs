use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    process,
    sync::Arc,
};

use futures_util::{stream::SplitSink, SinkExt};
use serde_json::{json, Value};
use tokio::sync::RwLock;
use warp::ws::{Message, WebSocket};

use crate::{utils, Users};

pub async fn connect(user_tx: &mut SplitSink<WebSocket, Message>) {
    let _ = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open("./static/info.txt");

    let prev_matches = fs::read_to_string("./static/info.txt").unwrap();

    if prev_matches.lines().count() > 0 {
        let initial_matches = json!({ "initial": prev_matches });

        user_tx
            .send(Message::text(initial_matches.to_string()))
            .await
            .unwrap();
    }
}

pub async fn disconnected(
    user_id: usize,
    users: &Users,
    files: Arc<RwLock<HashMap<usize, String>>>,
) {
    let file_to_delete = files.read().await;
    let file_to_delete = file_to_delete.get(&user_id);
    match file_to_delete {
        Some(file) => fs::remove_file(file).unwrap(),
        None => println!("No files found"),
    }

    eprintln!("User {} disconnected", user_id);

    users.write().await.remove(&user_id);
}

pub async fn request(user_id: usize, msg: Message, users: &Users) {
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

    let str_matches = utils::str_regex_matches(from, data);
    let prev_matches = fs::read_to_string("./static/info.txt").unwrap();
    let new_matches = if !prev_matches.is_empty() {
        let mut prev_matches = prev_matches.parse::<usize>().unwrap();
        prev_matches += str_matches;
        prev_matches
    } else {
        str_matches
    };

    let new_matches = String::from(new_matches.to_string());

    fs::write("./static/info.txt", &new_matches).expect("Error writing to file");

    let match_json = json!({
        "isOnlyMatches": true,
        "user_id": user_id,
        "matches": str_matches
    });

    let user_req_json = json!({
        "isOnlyMatches": false,
        "user_id": user_id,
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
