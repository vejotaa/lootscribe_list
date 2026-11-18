use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use reqwest::Client;
use serde::{Deserialize};
use serde_json::json;
use std::fs;
use std::path::Path;

use crate::prompt::SYSTEM_INSTRUCTION;

#[derive(Debug, Deserialize)]
pub struct ShopeeListing {
    pub titulo: String,
    pub descricao: String,
}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Option<Vec<Candidate>>,
}

#[derive(Deserialize)]
struct Candidate {
    content: Content,
}

#[derive(Deserialize)]
struct Content {
    parts: Vec<Part>,
}

#[derive(Deserialize)]
struct Part {
    text: String,
}

fn detect_mime_type(path: &Path) -> Result<&'static str> {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_lowercase())
        .as_deref()
        {
            Some("jpg" | "jpeg") => Ok("image/jpeg"),
            Some("png") => Ok("image/png"),
            Some("webp") => Ok("image/webp"),
            _ => bail!("formato de imagem não suportado. use PNG, JPG ou WEBP."),
        }
}

pub async fn generate_listing(
    api_key: &str,
    image_path: &Path,
    product_info: &str,
) -> Result<ShopeeListing> {
    let image_bytes = fs::read(image_path)
        .with_context(|| format!("falha ao ler a imagem em {:?}", image_path))?;
    let mime_type = detect_mime_type(image_path)?;
    let b64_image = STANDARD.encode(&image_bytes);

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateConcent?key={}",        api_key
    );

    let payload = json!({
        "system_instruction": {
            "parts": [{ "text": SYSTEM_INSTRUCTION }]
        },
        "contents": [{
            "parts": [
                {
                    "inline_data": {
                        "mime_type": mime_type,
                        "data": b64_image
                    }
                },
                {
                    "text": format!("metadados do produto para o anuncio:\n{}", product_info)
                }
            ]
        }],
        "generationConfig": {
            "temperature": 0.4,
            "responseMimeType": "application/json",
            "reponseSchema": {
                "type": "OBJECT",
                "properties": {
                    "titulo": {
                        "type": "STRING",
                        "description": "Título otimizado para SEO na Shopee com no máximo 100 caracteres."                      },
                    "descricao": {
                        "type": "STRING",
                        "description": "Descrição completa formatada com especificações, cuidados e hashtags."
                    }
                },
                "required": ["titulo", "descricao"]
            }
        }
    });

    let client = Client::new();
    let res = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .context("erro de rede ao conectar a API do gemini")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_body = res.text().await.unwrap_or_default();
        bail!("API erro: ({}): {}", status, err_body);
    }

    let parsed: GeminiResponse = res
        .json()
        .await
        .context("falha ao deserializar a resposta da API do gemini")?;

    let raw_json = parsed
        .candidates
        .and_then(|c| c.into_iter().next())
        .and_then(|c| c.content.parts.into_iter().next())
        .map(|p| p.text)
        .context("api não retornou conteudo valido")?;

    let listing: ShopeeListing = serde_json::from_str(&raw_json)
        .context("falha ao interpretar o JSON estruturado retornado pelo modelo")?;

    Ok(listing)
}

