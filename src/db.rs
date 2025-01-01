use sqlx::{SqlitePool, Row}; 

//Зберегти повідомлення у БД
pub async fn save_message(pool: &SqlitePool, username: &str, message: &str,) {
    if let Err(e) = sqlx::query!(
        "INSERT INTO messages (username, message) VALUES (?, ?)", username, message,
    )
        .execute(pool)
        .await
    {
        eprintln!("Error: can`t save message into database {}", e);
    }
}

//Отримати усі повідомлення з БД
pub async fn get_all_messages( pool: &SqlitePool,username: &str,) -> Result<Vec<(String, String, String)>, sqlx::Error> {
    let query = r#"
        SELECT username, message, strftime('%H:%M:%S %d.%m.%Y', timestamp) as time
        FROM messages
        ORDER BY timestamp ASC;
    "#;

    let rows = sqlx::query(query)
        .bind(username) 
        .bind(username)
        .fetch_all(pool)
        .await?;

    let messages = rows.into_iter().map(|row| {
            (
                row.get::<String, _>("username"),
                row.get::<String, _>("message"),
                row.get::<String, _>("time"),
            )
        })
        .collect();

    Ok(messages)
}