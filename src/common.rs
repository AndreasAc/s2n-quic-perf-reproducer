use std::{error::Error};

use bytes::Bytes;
use s2n_quic::stream::{ReceiveStream, SendStream};
use tokio::sync::watch;

pub async fn send_bytes_on_channel(
    send: &mut SendStream,
    to_send: u64,
    should_quit_receiver: watch::Receiver<bool>,
) -> Result<usize, Box<dyn Error>> {
    let mut data = s2n_quic_core::stream::testing::Data::new(to_send.try_into().unwrap());
    let mut chunks = vec![Bytes::new(); 64];

    loop {
        if *should_quit_receiver.borrow() {
            break;
        }

        match data.send(usize::MAX, &mut chunks) {
            Some(count) => {
                send.send_vectored(&mut chunks[..count]).await?;
            }
            None => {
                send.finish()?;
                break;
            }
        }
    }

    return Ok(data.offset().try_into().unwrap());
}

pub async fn read_all_from_channel(
    recv: &mut ReceiveStream,
    should_quit_receiver: watch::Receiver<bool>,
) -> Result<usize, Box<dyn Error>> {
    let mut received_data_bytes = 0;

    let mut chunks = vec![Bytes::new(); 64];

    loop {
        if *should_quit_receiver.borrow() {
            break;
        }

        let (len, is_open) = recv.receive_vectored(&mut chunks).await?;

        for chunk in chunks[..len].iter_mut() {
            received_data_bytes += chunk.len();
            *chunk = Bytes::new();
        }

        if !is_open {
            break;
        }
    }

    return Ok(received_data_bytes);
}
