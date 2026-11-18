mod cli;
mod clipboard;
mod gemini;
mod prompt;

use anyhow::Result;
use clap::Parser;
use std::io::{self, Write};

#[tokio::main]
async fn main() -> Result<()> {
    // carregar .env do dir atual
    dotenvy::dotenv().ok();

    let args = cli::Args::parse();

    println!("\n\n === analisando imagem e gerando anuncio via Gemini! ===\n");
    let listing = gemini::generate_listing(&args.api_key, &args.image, &args.info).await?;
    println!(
        "gerado com sucesso usando o modelo: [ {} ]\n",
        listing.model_used
    );
    println!(
        "\n==================== TÍTULO ({} chars) ====================",
        listing.titulo.chars().count()
    );
    println!("{}", listing.titulo);

    println!("\n======================= DESCRIÇÃO =======================");
    println!("{}", listing.descricao);
    println!("=========================================================\n");

    clipboard::copy_to_wayland(&listing.titulo)?;
    println!("[✓] TÍTULO copiado automaticamente para o clipboard (Wayland).");

    loop {
        print!("\nAções: [t] Copiar Título | [d] Copiar Descrição | [q] Sair > ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        match input.trim().to_lowercase().as_str() {
            "t" => {
                clipboard::copy_to_wayland(&listing.titulo)?;
                println!("[✓] Título copiado!");
            }
            "d" => {
                clipboard::copy_to_wayland(&listing.descricao)?;
                println!("[✓] Descrição copiada!");
            }
            "q" | "" => break,
            _ => println!("Opcao invalida."),
        }
    }

    Ok(())
}
