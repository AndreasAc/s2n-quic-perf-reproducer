use std::{
    error::Error,
    fs::File,
    io::BufWriter,
    net::{SocketAddr, ToSocketAddrs},
    path::Path,
    time::Instant,
};

use crate::{
    common::{read_all_from_channel, send_bytes_on_channel},
    recovery_metrics_logger::RecoveryMetricsLogger,
};
use bytesize::ByteSize;
use clap::Parser;
use log::{error, info};
use s2n_quic::{client::Connect, Client};
use tokio::{self, signal};

mod common;
mod recovery_metrics_logger;

/// Perf client used to investigate s2n-quic CC observations
#[derive(Parser, Debug)]
#[clap(about, version, author)]
struct Args {
    /// CC Logfile
    #[clap(short, long)]
    cc_logfile: Option<String>,
    #[clap(short, long)]
    remote: String,
    #[clap(long)]
    request_size: String,
    #[clap(long)]
    response_size: String,
    #[clap(long)]
    cert_file: String,
    #[clap(long)]
    disable_gso: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    let args = Args::parse();

    let amount_to_request: u64 = match args.response_size.parse::<ByteSize>() {
        Ok(parsed) => {
            if parsed.as_u64() > u64::MAX {
                let msg = format!(
                    "Can't request responses greater than than {}",
                    ByteSize(u64::MAX)
                );
                return Err(msg.into());
            } else {
                parsed.as_u64()
            }
        }
        Err(e) => {
            let msg = format!("Failed to parse response size, error: {}", e);
            return Err(msg.into());
        }
    };
    let amount_to_send: u64 = match args.request_size.parse::<ByteSize>() {
        Ok(parsed) => {
            if parsed.as_u64() < 8 {
                let msg = format!("Can't send less than 8 bytes application data per request due to protocol header size!");
                return Err(msg.into());
            } else {
                parsed.as_u64() - 8
            }
        }
        Err(e) => {
            let msg = format!("Failed to parse request size, error: {}", e);
            return Err(msg.into());
        }
    };

    let tls = s2n_quic::provider::tls::s2n_tls::Client::builder()
        .with_certificate(Path::new(&args.cert_file))?
        .with_application_protocols(vec!["perf"])?
        .build()?;

    let mut io_builder = s2n_quic::provider::io::tokio::Provider::builder();

    if args.disable_gso {
        info!("Disabling GSO");
        io_builder = io_builder.with_gso_disabled()?
    }

    let io = io_builder
        .with_receive_address("0.0.0.0:0".to_socket_addrs()?.next().unwrap())?
        .build()?;

    let client = match args.cc_logfile {
        Some(logfile_path) => {
            let file = File::create(logfile_path).unwrap();
            let logger = RecoveryMetricsLogger::new(Box::new(BufWriter::new(file)));
            Client::builder()
                .with_tls(tls)?
                .with_io(io)?
                .with_event(logger)?
                .start()?
        }

        None => Client::builder().with_tls(tls)?.with_io(io)?.start()?,
    };

    let addr: SocketAddr = args.remote.parse()?;
    let connect = Connect::new(addr).with_server_name("echo.test");

    let (exit_sender, mut quitting_receiver) = tokio::sync::watch::channel::<bool>(false);

    tokio::spawn(async move {
        let _ = signal::ctrl_c().await;
        info!("Received ctrl-c, sending exit.");
        let _ = exit_sender.send(true);
    });

    info!(
        "Perf-Client started (send size: {}, response size: {}).",
        ByteSize(amount_to_send).to_string_as(true),
        ByteSize(amount_to_request).to_string_as(true),
    );
    tokio::select! {
        Ok(mut connection) = client.connect(connect) => {
            info!("Connected.");
            'request_loop: loop {
                tokio::select! {
                    open_res = connection.open_bidirectional_stream() => {
                        if *quitting_receiver.borrow() {
                            break;
                        }

                        let (mut recv, mut send) = open_res.unwrap().split();

                        let send_start = Instant::now();

                        send.send(amount_to_request.to_be_bytes().to_vec().into()).await.unwrap();

                        // send the requested amount
                        let total_sent = send_bytes_on_channel(&mut send, amount_to_send, quitting_receiver.clone()).await.unwrap();

                        let send_duration = Instant::now() - send_start;

                        send.close().await.unwrap();

                        info!("Sent {} @{}it/s",
                            ByteSize(total_sent as u64).to_string_as(true),
                            ByteSize((total_sent as f32 * 8f32 / send_duration.as_millis() as f32 * 1000f32) as u64).to_string_as(false)
                        );

                        if *quitting_receiver.borrow() {
                            break;
                        }

                        let receive_start_time = Instant::now();

                        let received_data_bytes = read_all_from_channel(&mut recv, quitting_receiver.clone()).await.unwrap();

                        let receive_duration = Instant::now() - receive_start_time;

                        info!(
                            "Rcvd {} @{}it/s",
                            ByteSize(received_data_bytes as u64).to_string_as(true),
                            ByteSize((received_data_bytes as f32 * 8f32 / receive_duration.as_millis() as f32 * 1000f32) as u64).to_string_as(false)
                        );

                        if received_data_bytes != amount_to_request as usize && !*quitting_receiver.borrow() {
                            error!("Received mis-matching amount of response data! Received {} != {} requested!", received_data_bytes, amount_to_request);
                            break 'request_loop;
                        }
                    },
                    _ = quitting_receiver.changed() => {
                        if *quitting_receiver.borrow() {
                            info!("Received SIGINT, quitting.");
                            break 'request_loop;
                        }
                    }
                };
            }
        }

        _ = quitting_receiver.changed() => {
            info!("Received quit during connection setup, quitting.");
        }
    }

    return Ok(());
}
