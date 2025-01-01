mod db;

use db::{save_message, get_all_messages};
use sqlx::SqlitePool;
use futures_util::{stream::SplitSink, StreamExt, SinkExt};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use warp::Filter;
use tokio::sync::broadcast;
use warp::ws::Message;
use warp::http::StatusCode;
use serde::Deserialize;
use chrono::Utc;

type Users = Arc<Mutex<HashSet<String>>>; //Активні користувачі

#[derive(Deserialize)]
struct User {
    username: String,
    password: String,
}

#[tokio::main]
async fn main() {
    // Завантажуємо змінні середовища
    dotenv::dotenv().ok();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");// Отримуємо URL для підключення до бази даних
    let pool = Arc::new(SqlitePool::connect(&database_url).await.unwrap());// Створюємо пул з'єднань з базою даних
    let (tx, _rx) = broadcast::channel(100);// Створюємо канал для трансляції повідомлень
    let active_users: Users = Arc::new(Mutex::new(HashSet::new()));// Список активних користувачів
     // Клони пулу для різних частин коду
    let pool_clone1 = pool.clone(); 
    let pool_clone2 = pool.clone(); 
    // Маршрут для WebSocket з'єднання
    let chat_route = warp::path("ws")
        .and(warp::ws())
        .and(warp::query::<std::collections::HashMap<String, String>>())
        .and(warp::any().map(move || tx.clone()))
        .and(warp::any().map(move || pool_clone1.clone()))
        .and(warp::any().map(move || active_users.clone()))
        .map(|ws: warp::ws::Ws, params: std::collections::HashMap<String, String>, tx, pool, users| {
            let username = params.get("username").cloned().unwrap_or_default();
            let password = params.get("password").cloned().unwrap_or_default();
            ws.on_upgrade(move |socket| handle_socket(socket, tx, pool, users, username, password))
        });
    // Маршрут для логіну
    let login_route = warp::path("login")
        .and(warp::post())
        .and(warp::body::json::<User>())
        .and(warp::any().map(move || pool.clone()))
        .and_then(login_user);
    // Маршрут для реєстрації
    let register_route = warp::path("register")
        .and(warp::post())
        .and(warp::body::json::<User>())
        .and(warp::any().map(move || pool_clone2.clone()))
        .and_then(register_user);
    // Обробка статичних файлів
    let static_files = warp::fs::dir("./static");
    // Об'єднуємо всі маршрути
    let routes = static_files.or(chat_route).or(register_route).or(login_route);
    // Запускаємо сервер на порту 8080
    warp::serve(routes).run(([127, 0, 0, 1], 8080)).await;
}

async fn login_user(
    user_data: User,
    pool: Arc<SqlitePool>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let result = sqlx::query!(
        "SELECT * FROM users WHERE username = ? AND password = ?",
        user_data.username, 
        user_data.password
    )
    .fetch_optional(&*pool)
    .await;

    match result {
        Ok(Some(_user)) => {
            Ok(StatusCode::OK) 
        }
        Ok(None) => {
            Ok(StatusCode::UNAUTHORIZED) 
        }
        Err(e) => {
            eprintln!("Error during login: {}", e);
            Ok(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn register_user(
    user_data: User,
    pool: Arc<SqlitePool>,
) -> Result<impl warp::Reply, warp::Rejection> {
    let result = sqlx::query!(
        "INSERT INTO users (username, password) VALUES (?, ?)",
        user_data.username,
        user_data.password
    )
        .execute(&*pool)
        .await;

    match result {
        Ok(_) => Ok(StatusCode::CREATED),
        Err(e) => {
            eprintln!("Error registering user: {}", e);
            Ok(StatusCode::CONFLICT) 
        }
    }
}

// Обробка WebSocket-з'єднання
async fn handle_socket(
    socket: warp::ws::WebSocket, // WebSocket-з'єднання
    tx: broadcast::Sender<String>, // Канал для трансляції повідомлень
    pool: Arc<SqlitePool>, // Пул з'єднань з базою даних
    active_users: Users, // Список активних користувачів
    username: String, // Ім'я користувача
    _password: String, // Пароль користувача
) {
    // Розділення WebSocket-з'єднання на потоки для відправки і отримання повідомлень
    let (mut user_ws_tx, mut user_ws_rx) = socket.split();

    // Додавання користувача в список активних користувачів
    {
        let mut users = active_users.lock().unwrap();
        users.insert(username.clone());
    }

    // Відправка повідомлення про підключення користувача
    let join_message = format!("{} joined the chat", username);
    tx.send(join_message.clone()).unwrap();

    // Завантаження і відправка попередніх повідомлень з бази даних
    if let Ok(messages) = get_all_messages(&pool, &username).await {
        for (msg_username, msg_text, msg_time) in messages {
            let formatted_message = format!("{} ({}): {}", msg_username, msg_time, msg_text); 
            if let Err(e) = user_ws_tx.send(Message::text(formatted_message)).await {
                eprintln!("Failed to send message to {}: {}", username, e);
            }
        }
    } else {
        eprintln!("Failed to load messages for user: {}", username);
    }

    // Відправка списку активних користувачів клієнту
    send_active_users_to_client(&active_users, &mut user_ws_tx).await;
    // Трансляція оновленого списку активних користувачів всім підключеним
    broadcast_active_users(&active_users, &tx).await;

    // Асинхронне завдання для обробки повідомлень користувача
    let tx_for_disconnect = tx.clone();
    let tx_for_messages = tx.clone();
    let active_users_clone = active_users.clone();
    let username_clone = username.clone();
    let pool_clone = pool.clone();

    tokio::spawn(async move {
        while let Some(result) = user_ws_rx.next().await {
            if let Ok(msg) = result {
                if msg.is_text() {
                    // Форматування і відправка нового повідомлення
                    let text = msg.to_str().unwrap();
                    let timestamp = Utc::now().format("%H:%M:%S %d.%m.%Y").to_string();

                    let public_message = format!("{} ({}): {}", username_clone, timestamp, text);
                    save_message(&pool_clone, &username_clone, text).await; // Збереження повідомлення в базі даних
                    tx_for_messages.send(public_message).unwrap(); // Трансляція повідомлення всім користувачам
                }
            }
        }

        // Видалення користувача зі списку активних при відключенні
        {
            let mut users = active_users_clone.lock().unwrap();
            users.remove(&username_clone);
        }

        // Відправка повідомлення про вихід користувача з чату
        tx_for_disconnect
            .send(format!("{} left the chat", username_clone))
            .unwrap();

        // Трансляція оновленого списку активних користувачів
        broadcast_active_users(&active_users_clone, &tx_for_disconnect).await;
    });

    // Асинхронне завдання для прослуховування і відправки повідомлень всім користувачам
    tokio::spawn(async move {
        let mut rx = tx.subscribe();
        while let Ok(msg) = rx.recv().await {
            // Відправка трансляційного повідомлення клієнту
            if let Err(e) = user_ws_tx.send(Message::text(msg)).await {
                eprintln!("Failed to send message: {}", e);
                break;
            }
        }
    });
}

// Відправка списку активних користувачів клієнту
async fn send_active_users_to_client(users: &Users, ws_tx: &mut SplitSink<warp::ws::WebSocket, Message>) {
    let users_list: Vec<String> = users.lock().unwrap().iter().cloned().collect(); // Формування списку користувачів
    let message = format!("ACTIVE_USERS: {}", serde_json::to_string(&users_list).unwrap()); // Формування повідомлення
    if let Err(e) = ws_tx.send(Message::text(message)).await { // Відправка повідомлення через WebSocket
        eprintln!("Failed to send active users to client: {}", e);
    }
}

// Трансляція списку активних користувачів всім підключеним
async fn broadcast_active_users(users: &Users, tx: &broadcast::Sender<String>) {
    let users_list: Vec<String> = users.lock().unwrap().iter().cloned().collect(); // Формування списку користувачів
    let message = format!("ACTIVE_USERS: {}", serde_json::to_string(&users_list).unwrap()); // Формування повідомлення
    tx.send(message).unwrap(); // Відправка трансляції через канал
    println!("Broadcasting active users: {}", serde_json::to_string(&users_list).unwrap()); // Логування
}
