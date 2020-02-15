use async_std::task;
use futures::stream::StreamExt;
use kiss_queue::mpsc;
use std::{error::Error, result::Result, time::Duration};

fn main() -> Result<(), Box<dyn Error>> {
    let (sender, mut receiver) = mpsc::<(u32, u32)>();

    for i in 0..10 {
        let sender = sender.clone();
        task::spawn(async move {
            for j in 0..10 {
                println!("sending {:?}", (i, j));
                async_std::task::sleep(Duration::from_secs(1)).await;
                sender.send((i, j)).unwrap();
            }
        });
    }
    std::mem::drop(sender);

    task::block_on(async move {
        while let Some(value) = receiver.next().await {
            println!("{:?}", value);
        }
    });

    Ok(())
}
