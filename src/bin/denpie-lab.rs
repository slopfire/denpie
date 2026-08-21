#[tokio::main]
async fn main() {
    let code = denpie::lab::run(std::env::args().skip(1).collect()).await;
    std::process::exit(code);
}
