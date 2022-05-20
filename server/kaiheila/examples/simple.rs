use kaiheila::message::Event;
use kaiheila::Client;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let client = Client::new_builder()
        .set_guild_id("3866649617797325")
        .set_channel_id("1369745483268903")
        .build()
        .await
        .unwrap();

    let mut stream = client.subscribe(Event::AudioChannelUserTalk).await.unwrap();

    while let Some(msg) = stream.recv().await {
        println!("{:?}", msg.get_data_array())
    }
}
