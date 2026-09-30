use data_rs::{
    components::email::Emailer,
    models::{currencies::Currencies, dex::Dex, meta::Meta, waitlist::Waitlist},
    server::run_server,
    utils::zilliqa::Zilliqa,
};
use log::{error, LevelFilter};
use simple_logger::SimpleLogger;
use std::{sync::Arc, time::Duration};
use tokio;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() {
    SimpleLogger::new()
        .with_colors(true)
        .with_level(LevelFilter::Info)
        .init()
        .unwrap();

    let db_path = std::env::var("DB_PATH").expect("Incorrect DB_PATH env var");
    let port: u16 = std::env::var("PORT")
        .expect("ENV var PORT is required")
        .parse()
        .expect("ENV var PORT should be u16 number");

    let meta = Arc::new(RwLock::new(Meta::new(&db_path)));
    let rates = Arc::new(RwLock::new(Currencies::new(&db_path)));
    let dex = Arc::new(RwLock::new(Dex::new(&db_path)));
    let waitlist = Arc::new(Waitlist::new(&db_path));
    let emailer = Arc::new(Emailer::new());

    let meta_ref = Arc::clone(&meta);
    let dex_ref = Arc::clone(&dex);
    let meta_dex_ref = Arc::clone(&dex);
    let rates_ref = Arc::clone(&rates);

    tokio::task::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(50)).await;

            let zil = Zilliqa::new();
            let tokens = match Meta::get_meta_tokens().await {
                Ok(tokens) => tokens,
                Err(e) => {
                    error!("github:meta: {:?}", e);

                    continue;
                }
            };

            let sorted = match Meta::sort_zilliqa_tokens(&tokens, &zil).await {
                Ok(sorted) => sorted,
                Err(e) => {
                    error!("zilliqa node: {:?}", e);

                    continue;
                }
            };

            let mut unwarp_meta = meta_ref.write().await;

            match unwarp_meta.update(tokens, sorted) {
                Ok(_) => {
                    let dex = meta_dex_ref.read().await;

                    unwarp_meta.listed_tokens_update(&dex);
                    unwarp_meta.write_db().unwrap(); // TODO: make Error hanlder.
                }
                Err(e) => {
                    error!("tokens update: {:?}", e);

                    continue;
                }
            };
        }
    });

    tokio::task::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(20)).await;

            match Currencies::fetch_rates().await {
                Ok(rates) => {
                    rates_ref.write().await.update(rates).unwrap();
                }
                Err(e) => {
                    error!("fetch rates error: {:?}", e);
                }
            };
        }
    });

    tokio::task::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(20)).await;

            let zil = Zilliqa::new();
            match Dex::get_pools(&zil).await {
                Ok(pools) => {
                    dex_ref.write().await.update(pools).unwrap();
                }
                Err(e) => {
                    error!("fetch rates error: {:?}", e);
                }
            };
        }
    });

    let meta_ref0 = Arc::clone(&meta);
    let dex_ref0 = Arc::clone(&dex);
    let rates_ref0 = Arc::clone(&rates);

    run_server(
        &meta_ref0,
        &dex_ref0,
        &rates_ref0,
        &waitlist,
        &emailer,
        port,
    )
    .await
    .unwrap();
}
