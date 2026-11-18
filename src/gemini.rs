use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::fs;
use std::path::Path;
use std::time::Duration;

use crate::prompt::SYSTEM_INSTRUCTION;

#[derive(Debug, Deserialize)]
pub struct ShopeeListing {
    pub titulo: String,
    pub descricao: String,
    #[serde(skip)]
    pub model_used: String,
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
        _ => bail!("Formato de imagem não suportado. Use PNG, JPG ou WEBP."),
    }
}

pub async fn generate_listing(
    api_key: &str,
    image_path: &Path,
    product_info: &str,
) -> Result<ShopeeListing> {
    let image_bytes = fs::read(image_path)
        .with_context(|| format!("Falha ao ler a imagem em {:?}", image_path))?;
    let mime_type = detect_mime_type(image_path)?;
    let b64_image = STANDARD.encode(&image_bytes);

    let prompt_text = format!(
        "Metadados do produto para o anúncio:\n{}\n\n\
        Retorne APENAS um objeto JSON válido com exatamente estas duas chaves de string: \
        \"titulo\" e \"descricao\". Não inclua blocos markdown (```json) nem texto fora do JSON.",
        product_info
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
                    "text": prompt_text
                }
            ]
        }],
        "generationConfig": {
            "temperature": 0.4,
            "responseMimeType": "application/json"
        }
    });

    let models = [
        "gemini-3.5-flash-lite",
        "gemini-3.5-flash",
        "gemini-3.8-flash",
        "gemini-3.6-flash",
        "gemini-flash-latest",
    ];

    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("Falha ao construir cliente HTTP")?;

    let mut last_error = String::new();

    for model in models {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
            model.trim()
        );

        let res = match client
            .post(&url)
            .header("x-goog-api-key", api_key.trim())
            .json(&payload)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                last_error = format!("Erro de rede em {}: {:?}", model, e);
                continue;
            }
        };

        let status = res.status();
        if status.is_success() {
            let parsed: GeminiResponse = res
                .json()
                .await
                .context("Falha ao deserializar a resposta da API do Gemini")?;

            let raw_json = parsed
                .candidates
                .and_then(|c| c.into_iter().next())
                .and_then(|c| c.content.parts.into_iter().next())
                .map(|p| p.text)
                .context("A API não retornou conteúdo válido")?;

            let clean_json = raw_json
                .trim()
                .trim_start_matches("```json")
                .trim_start_matches("```")
                .trim_end_matches("```")
                .trim();

            let mut listing: ShopeeListing =
                serde_json::from_str(clean_json).with_context(|| {
                    format!("JSON inválido retornado por {}: {}", model, clean_json)
                })?;

            listing.model_used = model.to_string();

            return Ok(listing);
        }

        let err_body = res.text().await.unwrap_or_default();
        eprintln!(
            "[!] {} falhou ({}), tentando próximo modelo...",
            model, status
        );
        last_error = format!("({}) {}", status, err_body);
    }

    bail!("Todos os modelos falharam. Último erro: {}", last_error)
}
