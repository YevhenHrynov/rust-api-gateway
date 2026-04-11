use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
#[command(name = "api-gateway", about = "High-Performance API Gateway")]
pub struct Cli {
    #[arg(long, default_value = "gateway/config")]
    pub config_dir: PathBuf,

    #[arg(long)]
    pub host: Option<String>,

    #[arg(short, long)]
    pub port: Option<u16>,
}
