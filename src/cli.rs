use clap::{ArgGroup, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "lootscribe_list",
    version,
    about = "Gerador de títulos e descrições Shopee para miniaturas 3D",
    args_conflicts_with_subcommands = true
)]
#[command(group(
    ArgGroup::new("reference")
        .args(["image", "url", "info"])
        .multiple(true)
))]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Caminho da imagem da miniatura (PNG, JPG, WEBP)
    #[arg(short, long)]
    pub image: Option<PathBuf>,

    /// URL de referência do modelo (MakerWorld, Printables, etc.)
    #[arg(short, long)]
    pub url: Option<String>,

    /// Metadados adicionais (altura, acabamento, observações)
    #[arg(short = 'm', long)]
    pub info: Option<String>,

    /// Chave da API Gemini (padrão: env GEMINI_API_KEY)
    #[arg(
        long = "api-key",
        env = "GEMINI_API_KEY",
        hide_env_values = true,
        global = true
    )]
    pub api_key: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Processa múltiplos anúncios em lote (via arquivo ou inserção dinâmica)
    Batch {
        /// Arquivo .txt com uma URL por linha (opcional: `url | metadados`)
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// Intervalo mínimo (segundos) entre requisições (padrão: 8)
        #[arg(long, default_value_t = 8)]
        min_delay: u64,

        /// Intervalo máximo (segundos) entre requisições (padrão: 20)
        #[arg(long, default_value_t = 20)]
        max_delay: u64,
    },
    /// Abre a interface interativa TUI (padrão ao executar sem argumentos)
    Tui {
        /// Arquivo .txt opcional para pré-carregar a fila (`url` ou `url | metadados`)
        #[arg(short, long)]
        file: Option<PathBuf>,
    },
}
