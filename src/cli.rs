use clap::{ArgGroup, Parser};
use std::path::PathBuf;
#[derive(Parser, Debug)]
#[command(
    name = "lootscribe-list",
    about = "Gera títulos e descrições otimizados para miniaturas 3D na Shopee via Gemini",
    group(
        ArgGroup::new("reference")
        .required(true)
        .multiple(true)
        .args(["image", "url", "info"])
    )
)]
pub struct Args {
    /// Caminho p foto da miniatura/peça (JPG, PNG, WEBP)
    #[arg(short = 'i', long = "image")]
    pub image: Option<PathBuf>,

    /// URL
    #[arg(short = 'u', long = "url")]
    pub url: Option<String>,

    /// Metadados do produto (personagem, franquia, acabamento, tamanhos/variações)
    #[arg(short = 'm', long = "metadata")]
    pub info: Option<String>,

    /// Chave da API do Google AI Studio (GEMINI_API_KEY)
    #[arg(long = "api-key", env = "GEMINI_API_KEY", hide_env_values = true)]
    pub api_key: String,
}
