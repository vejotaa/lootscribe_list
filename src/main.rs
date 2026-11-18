use anyhow::Result;
use clap::Parser;
use lootscribe_list::{ClipboardManager, ListingRequest, generate_listing};
use std::io::{self, Write};

mod cli;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    let api_key = cli.api_key.ok_or_else(|| {
        anyhow::anyhow!(
            "GEMINI_API_KEY nao encontrada, defina no arquivo .env ou passe via --api-key"
        )
    })?;
    let mut clipboard = ClipboardManager::new()?;

    match cli.command {
        Some(Commands::Batch {
            file,
            min_delay,
            max_delay,
        }) => {
            lootscribe_list::batch::run_batch_mode(
                api_key,
                file,
                min_delay,
                max_delay,
                &mut clipboard,
            )
            .await?;
        }
        None => {
            let request = ListingRequest {
                image: cli.image,
                url: cli.url,
                info: cli.info,
            };
            request.validate()?;

            println!("\n=== analisando imagem e gerando anuncio via Gemini! ===\n");
            let listing = generate_listing(&api_key, &request).await?;

            println!(
                "gerado com sucesso usando o modelo: [ {} ]\n",
                listing.model_used
            );
            println!(
                "\n=============== TÍTULO ({} chars) ===============\n{}",
                listing.titulo.chars().count(),
                listing.titulo
            );
            println!(
                "\n=============== DESCRIÇÃO ===============\n{}",
                listing.descricao
            );
            println!("=========================================\n");

            clipboard.copy(&listing.titulo)?;
            println!("[✓] TÍTULO copiado automaticamente para o clipboard (Wayland).");

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
        }
    }

    Ok(())
}
