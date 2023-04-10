use std::{collections::HashMap, sync::Arc};

use tokio::sync::{mpsc, RwLock};
use warp::ws::Message;

pub mod user;

pub type Users = Arc<RwLock<HashMap<usize, mpsc::UnboundedSender<Message>>>>;
pub type Files = Arc<RwLock<HashMap<usize, std::string::String>>>;
