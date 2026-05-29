use zbus::Connection;

pub async fn system_connection() -> zbus::Result<Connection> {
    Connection::system().await
}
