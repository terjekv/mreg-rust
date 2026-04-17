use std::io;

#[actix_web::main]
async fn main() -> io::Result<()> {
    mreg_rust::run().await
}
