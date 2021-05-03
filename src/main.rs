use anyhow::Result;
use async_io::Async;
use futures::future::FutureExt;
use futures::stream::StreamExt;
use pcap::{Capture, Device, Error};
use std::io;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt)]
struct Opts {
    #[structopt(short, long, conflicts_with("output"))]
    input: Option<PathBuf>,
    #[structopt(short, long, conflicts_with("input"))]
    output: Option<PathBuf>,
    #[structopt(short, long)]
    device: Option<String>,
    #[structopt(short, long, default_value = "1000000")]
    buffer_size: usize,
    #[structopt(short, long, default_value = "0")]
    timeout: usize,
    #[structopt(short, long)]
    promisc: bool,
    #[structopt(short, long)]
    rfmon: bool,
    #[structopt(long)]
    immediate: bool,
    #[structopt(short, long)]
    verbose: bool,
}

#[async_std::main]
async fn main() -> Result<()> {
    env_logger::init();
    let opts = Opts::from_args();

    if let Some(device) = opts.device {
        let device = Capture::from_device(device.as_str())?
            .buffer_size(opts.buffer_size as i32)
            .timeout(opts.timeout as i32)
            .promisc(opts.promisc)
            .rfmon(opts.rfmon)
            .immediate_mode(opts.immediate)
            .open()?;
        let mut device = Async::new(device.setnonblock()?)?;
        if let Some(o) = opts.output.as_ref() {
            let (tx, mut rx) = async_channel::bounded(100);
            ctrlc::set_handler(move || {
                tx.try_send(()).ok();
            })?;

            let mut output = device.get_ref().savefile(&o)?;
            println!("writing to {}", o.display());
            loop {
                futures::select! {
                    ev = device.read_with_mut(|device| match device.next() {
                        Ok(packet) => {
                            output.write(&packet);
                            Ok(Some(packet.data.len()))
                        }
                        Err(Error::IoError(io::ErrorKind::Interrupted)) |
                        Err(Error::IoError(io::ErrorKind::WouldBlock)) |
                        Err(Error::PcapError(_)) |
                        Err(Error::TimeoutExpired) => Err(io::Error::new(io::ErrorKind::WouldBlock, anyhow::anyhow!(""))),
                        Err(Error::IoError(kind)) => Err(io::Error::new(kind, anyhow::anyhow!(""))),
                        Err(err) => Err(io::Error::new(io::ErrorKind::Other, err)),
                    }).fuse() => {
                        if let Some(len) = ev? {
                            if opts.verbose {
                                println!("captured {} bytes", len);
                            }
                        } else {
                            break;
                        }
                    }
                    _ = rx.next().fuse() => break,
                }
            }
        } else if let Some(i) = opts.input.as_ref() {
            let mut input = Capture::from_file(i)?;
            println!("reading from {}", i.display());
            loop {
                match input.next() {
                    Ok(packet) => {
                        if opts.verbose {
                            println!("sending {} bytes", packet.data.len());
                        }
                        device.get_mut().sendpacket(&*packet)?;
                    }
                    Err(Error::NoMorePackets) => break,
                    Err(err) => return Err(err.into()),
                }
            }
        } else {
            anyhow::bail!("required input or output");
        }
        let stats = device.get_mut().stats()?;
        println!("received {}", stats.received);
        println!("dropped {}", stats.dropped);
        println!("if_dropped {}", stats.if_dropped);
    } else {
        for device in Device::list()? {
            print!("{:30}", device.name);
            if let Some(desc) = device.desc.as_ref() {
                print!(" {}", desc);
            }
            println!();
        }
    }

    Ok(())
}
