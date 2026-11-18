use clap::Parser;
use std::path::PathBuf;
#[derive(Parser, Debug)]
#[command(name = "shopee-gen", about = "Gerador de anúncios da Shopee para impressão 3D")]

pub struct Args {
    /// Caminho p foto da miniatura/peça (JPG, PNG, WEBP)
    #[arg(short, long)]
    pub image: PathBuf,

    /// Metadados do produto (personagem, franquia, acabamento, tamanhos/variações)
    #[arg(short, long)]
    pub info: String,

    /// Chave da API do Google AI Studio (GEMINI_API_KEY)
    #[arg(long, env = "GEMINI_API_KEY", hide_env_values = true)]
    pub api_key: String,
}
