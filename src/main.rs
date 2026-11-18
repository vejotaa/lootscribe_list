mod cli;

use anyhow::Result;
use clap::Parser;
use lootscribe_list::{ClipboardManager, generate_listing};
use std::io::{self, Write};

#[tokio::main]
async fn main() -> Result<()> {
    // carregar .env do dir atual
    dotenvy::dotenv().ok();

    let args = cli::Args::parse();

    // instance viva durante toda exec do programa p garantir persist no Wayland
    let mut clipboard = ClipboardManager::new()?;

    let request = lootscribe_list::ListingRequest {
        image: args.image,
        url: args.url,
        info: args.info,
    };

    println!("\n\n === analisando imagem e gerando anuncio via Gemini! ===\n");
    let listing = generate_listing(&args.api_key, &request).await?;

    println!(
        "gerado com sucesso usando o modelo: [ {} ]\n",
        listing.model_used
    );
    println!(
        "\n==================== TÍTULO ({} chars) ====================\n{}",
        listing.titulo.chars().count(),
        listing.titulo
    );

    println!(
        "\n======================= DESCRIÇÃO =======================\n{}",
        listing.descricao
    );
    println!("=========================================================\n");

    clipboard.copy(&listing.titulo)?;
    println!("[✓] TÍTULO copiado automaticamente para o clipboard (Wayland).");

    // menu interativo (segurando clip no wayland)
    loop {
        print!("\nAções: [t] Copiar Título | [d] Copiar Descrição | [q] Sair > ");
        io::stdout().flush()?;

        let mut choice = String::new();
        io::stdin().read_line(&mut choice)?;

        match choice.trim().to_lowercase().as_str() {
            "t" => {
                clipboard.copy(&listing.titulo)?;
                println!("[✓] Título copiado!");
            }
            "d" => {
                clipboard.copy(&listing.descricao)?;
                println!("[✓] Descrição copiada!");
            }
            "q" | "" => break,
            _ => println!("Opcao invalida."),
        }
    }

    Ok(())
}
